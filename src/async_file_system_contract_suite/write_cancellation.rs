// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Stage-aware cancellation of owning asynchronous writes.

use std::future::Future;
use std::future::poll_fn;
use std::task::Poll;

use qubit_fs::error::FsErrorKind;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::write::AsyncWriteAllOperationState;
use qubit_fs::write::WriteAbortOutcome;
use qubit_fs::write::WriteFailureState;
use qubit_fs::write::WriterState;

use crate::AsyncFileSystemContractSuite;
use crate::AsyncWriteCancellationStage;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixtureSupport;
use crate::WriteCancellationProbe;
use crate::internal::probe_disarm_guard::ProbeDisarmGuard;
use crate::internal::verify_condition;

impl AsyncFileSystemContractSuite<'_> {
    /// Executes only the selected stage, including its own probe preparation.
    pub(super) async fn check_write_cancellation_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        let stage = match id {
            ContractCheckId::WriteCancelOpen => AsyncWriteCancellationStage::Open,
            ContractCheckId::WriteCancelWrite => AsyncWriteCancellationStage::Write,
            ContractCheckId::WriteCancelFlush => AsyncWriteCancellationStage::Flush,
            ContractCheckId::WriteCancelCommit => AsyncWriteCancellationStage::Commit,
            _ => return Err(ContractFailure::message_only("selected entry is not write cancellation").at(id)),
        };
        if !self.capable(FileSystemCapability::Write) {
            self.context.record_check(
                id,
                Some(FileSystemCapability::Write),
                ContractCheckOutcome::NotApplicable {
                    reason: "Write capability is unavailable".to_owned(),
                },
            );
            return Ok(());
        }
        let relative = self.context.relative_name(&format!("write-cancel-{stage:?}"));
        let prepared = self
            .fixture
            .prepare_write_cancellation(stage, &relative)
            .await
            .map_err(|error| ContractFailure::with_source("write cancellation preparation failed", error).at(id))?;
        match prepared {
            FixtureSupport::Supported(probe) => self.run_write_cancellation_probe(stage, id, probe).await?,
            FixtureSupport::Unsupported => self.context.record_check(
                id,
                Some(FileSystemCapability::Write),
                ContractCheckOutcome::SkippedOptional {
                    reason: "fixture has no stage acknowledgement probe".to_owned(),
                },
            ),
        }
        Ok(())
    }

    async fn run_write_cancellation_probe(
        &mut self,
        stage: AsyncWriteCancellationStage,
        id: ContractCheckId,
        probe: Box<dyn WriteCancellationProbe>,
    ) -> Result<(), ContractFailure> {
        let (path, bytes, options) = probe.case().clone().into_parts();
        self.context.record_created(path.clone());
        // The observation can suspend too: own the disarm obligation before it.
        // Later locals drop first, so execution always stops before disarming.
        let mut guard = ProbeDisarmGuard::new(|| probe.disarm(), &mut self.context.run.cleanup.failures);
        let before = probe.observe_target().await.map_err(|error| {
            ContractFailure::with_source("write cancellation initial observation failed", error).at(id)
        })?;
        let mut operation = self
            .fixture
            .file_system()
            .begin_write_all(path, bytes.clone(), options)
            .map_err(|error| {
                ContractFailure::with_owned_source(
                    "write cancellation request admission failed",
                    crate::ContractAsyncWriteFailure::new(error, None),
                )
                .at(id)
            })?;
        drop(operation.execute());
        verify_condition(
            operation.state() == AsyncWriteAllOperationState::Ready,
            id,
            "unpolled execution changed state",
        )?;
        let mut execution_failure = None;
        let mut execution = Box::pin(operation.execute());
        let reached = poll_fn(|context| match execution.as_mut().poll(context) {
            Poll::Pending => probe.poll_reached(context).map(|result| {
                result.map_err(|error| {
                    ContractFailure::with_source("write cancellation stage acknowledgement failed", error).at(id)
                })
            }),
            Poll::Ready(Ok(_)) => Poll::Ready(Err(ContractFailure::message_only(
                "write completed before requested stage acknowledgement",
            )
            .at(id))),
            Poll::Ready(Err(error)) => {
                execution_failure = Some(error);
                Poll::Ready(Err(ContractFailure::message_only(
                    "write failed before requested stage acknowledgement",
                )
                .at(id)))
            }
        })
        .await;
        drop(execution);
        let disarmed = guard.disarm();
        drop(guard);
        if let Some(error) = execution_failure {
            return Err(ContractFailure::with_owned_source(
                "write failed before requested stage acknowledgement",
                crate::ContractAsyncWriteFailure::new(error, Some(operation)),
            )
            .at(id));
        }
        reached?;
        verify_condition(
            disarmed,
            id,
            "probe disarm failed; original cause is retained in cleanup",
        )?;
        let expected_bytes = probe.accepted_bytes().map_err(|error| {
            ContractFailure::with_source("write cancellation byte observation failed", error).at(id)
        })?;
        verify_condition(
            expected_bytes <= bytes.len() as u64,
            id,
            "observed accepted bytes exceed the request",
        )?;
        verify_condition(
            operation.state() == AsyncWriteAllOperationState::Failed(WriteFailureState::Indeterminate),
            id,
            "cancellation state mismatch",
        )?;
        verify_condition(
            operation.written_bytes() == expected_bytes,
            id,
            "accepted byte count mismatch",
        )?;
        verify_condition(
            operation.has_recovery_writer() == (stage != AsyncWriteCancellationStage::Open),
            id,
            "writer recovery responsibility mismatch",
        )?;
        let failure = match operation.execute().await {
            Err(failure) => failure,
            Ok(_) => return Err(ContractFailure::message_only("cancelled write ran twice").at(id)),
        };
        if failure.error().kind() != FsErrorKind::InvalidState
            || failure.state() != WriteFailureState::Indeterminate
            || failure.written_bytes() != expected_bytes
        {
            return Err(ContractFailure::with_owned_source(
                "repeated cancelled write lost its recovery facts",
                crate::ContractAsyncWriteFailure::new(failure, Some(operation)),
            )
            .at(id));
        }
        verify_condition(
            operation.has_recovery_writer() == (stage != AsyncWriteCancellationStage::Open),
            id,
            "repeated execution lost writer",
        )?;
        if let Some(mut writer) = operation.take_recovery_writer() {
            let outcome = match writer.abort_async().await {
                Ok(outcome) => outcome,
                Err(error) => {
                    return Err(ContractFailure::with_owned_source(
                        "write cancellation recovery abort failed",
                        crate::ContractWriterFailure::new(error, writer),
                    )
                    .at(id));
                }
            };
            let expected = match outcome {
                WriteAbortOutcome::NotPublished => WriterState::Aborted,
                WriteAbortOutcome::Published => WriterState::Published,
                WriteAbortOutcome::Indeterminate => WriterState::Indeterminate,
            };
            verify_condition(writer.state() == expected, id, "recovery state mismatch")?;
            if outcome != WriteAbortOutcome::Indeterminate {
                let after = probe.observe_target().await.map_err(|error| {
                    ContractFailure::with_source("write cancellation recovery observation failed", error).at(id)
                })?;
                match outcome {
                    WriteAbortOutcome::NotPublished => {
                        verify_condition(after == before, id, "NotPublished recovery changed the target")?
                    }
                    WriteAbortOutcome::Published => verify_condition(
                        after.as_deref() == Some(bytes.as_slice()),
                        id,
                        "confirmed publication differs",
                    )?,
                    WriteAbortOutcome::Indeterminate => unreachable!(),
                }
            }
        }
        self.context
            .record_check(id, Some(FileSystemCapability::Write), ContractCheckOutcome::Passed);
        Ok(())
    }
}
