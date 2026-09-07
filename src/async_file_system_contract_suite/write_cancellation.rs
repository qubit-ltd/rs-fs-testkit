// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Explicit evidence for owning writes and provider-observed cancellation.

use std::future::Future;
use std::future::poll_fn;
use std::panic::resume_unwind;
use std::task::Poll;

use qubit_fs::error::FsErrorKind;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::write::AsyncWriteAllOperationState;
use qubit_fs::write::WriteAbortOutcome;
use qubit_fs::write::WriteFailureState;
use qubit_fs::write::WriteOptions;
use qubit_fs::write::WriterState;

use super::AsyncFileSystemContractSuite;
use crate::AsyncWriteCancellationStage;
use crate::ContractCheckOutcome;
use crate::FixtureError;
use crate::FixtureSupport;
use crate::WriteCancellationProbe;
use crate::internal::catch_unwind_future;

impl AsyncFileSystemContractSuite<'_> {
    /// Exercises completed ownership and rejects repeated execution.
    pub(super) async fn assert_owning_write(&mut self) {
        let path = self.path("async-owned-write");
        let prepared = self
            .fixture
            .file_system()
            .begin_write_all(path.clone(), Vec::new(), WriteOptions::default());
        if !self.capable(FileSystemCapability::Write) {
            let failure = match prepared {
                Ok(_) => panic!("write/owning-operation: missing capability accepted"),
                Err(failure) => failure,
            };
            assert_eq!(FsErrorKind::UnsupportedCapability, failure.error().kind());
            self.write_check("write/owning-operation", ContractCheckOutcome::RejectedAsExpected);
            self.write_check(
                "write/repeated-execute",
                ContractCheckOutcome::NotApplicable {
                    reason: "write capability unavailable".to_owned(),
                },
            );
            return;
        }
        self.context.record_created(path.clone());
        let mut operation = prepared.expect("write/owning-operation: preflight failed");
        assert_eq!(&path, operation.path());
        operation
            .execute()
            .await
            .expect("write/owning-operation: empty write failed");
        assert_eq!(AsyncWriteAllOperationState::Completed, operation.state());
        assert_eq!(0, operation.written_bytes());
        assert!(!operation.has_recovery_writer());
        self.write_check("write/owning-operation", ContractCheckOutcome::Passed);
        let failure = operation
            .execute()
            .await
            .expect_err("write/repeated-execute: operation ran twice");
        assert_eq!(FsErrorKind::InvalidState, failure.error().kind());
        assert_eq!(WriteFailureState::Published, failure.state());
        assert_eq!(AsyncWriteAllOperationState::Completed, operation.state());
        assert_eq!(0, operation.written_bytes());
        self.write_check("write/repeated-execute", ContractCheckOutcome::Passed);
    }

    /// Requires a real stage gate whenever write capability is advertised.
    pub(super) async fn assert_write_cancellation(&mut self) {
        for stage in [
            AsyncWriteCancellationStage::Open,
            AsyncWriteCancellationStage::Write,
            AsyncWriteCancellationStage::Flush,
            AsyncWriteCancellationStage::Commit,
        ] {
            let id = check_id(stage);
            if !self.capable(FileSystemCapability::Write) {
                self.write_check(
                    id,
                    ContractCheckOutcome::NotApplicable {
                        reason: "write capability unavailable".to_owned(),
                    },
                );
                continue;
            }
            let relative = self.context.relative_name(id);
            match self
                .fixture
                .prepare_write_cancellation(stage, &relative)
                .await
                .expect("write cancellation: probe setup failed")
            {
                FixtureSupport::Supported(probe) => self.run_write_probe(stage, probe).await,
                FixtureSupport::Unsupported => self.write_check(
                    id,
                    ContractCheckOutcome::Unverified {
                        reason: "provider did not supply a stage-aware write cancellation probe".to_owned(),
                    },
                ),
            }
        }
    }

    /// Polls until acknowledgement, cancels execution, and explicitly recovers.
    async fn run_write_probe(&mut self, stage: AsyncWriteCancellationStage, probe: Box<dyn WriteCancellationProbe>) {
        let (path, bytes, options) = probe.case().clone().into_parts();
        self.context.record_created(path.clone());
        let expected_bytes = bytes.len() as u64;
        let result = catch_unwind_future(async {
            let mut operation = self
                .fixture
                .file_system()
                .begin_write_all(path, bytes, options)
                .expect("write cancellation: preflight failed");
            let mut execution = Box::pin(operation.execute());
            let reached = poll_fn(|context| match execution.as_mut().poll(context) {
                Poll::Pending => probe.poll_reached(context),
                Poll::Ready(_) => Poll::Ready(Err(FixtureError::new(
                    "write cancellation: execution ended before stage acknowledgement",
                ))),
            })
            .await;
            drop(execution);
            reached.expect("write cancellation: stage acknowledgement failed");
            assert_eq!(
                AsyncWriteAllOperationState::Failed(WriteFailureState::Indeterminate),
                operation.state()
            );
            let confirmed = operation.written_bytes();
            match stage {
                AsyncWriteCancellationStage::Open => {
                    assert_eq!(0, confirmed);
                    assert!(!operation.has_recovery_writer());
                }
                AsyncWriteCancellationStage::Write => {
                    assert!(confirmed <= expected_bytes);
                    assert!(operation.has_recovery_writer());
                }
                AsyncWriteCancellationStage::Flush | AsyncWriteCancellationStage::Commit => {
                    assert_eq!(expected_bytes, confirmed);
                    assert!(operation.has_recovery_writer());
                }
            }
            let failure = operation
                .execute()
                .await
                .expect_err("write cancellation: repeated execution accepted");
            assert_eq!(FsErrorKind::InvalidState, failure.error().kind());
            assert_eq!(WriteFailureState::Indeterminate, failure.state());
            assert_eq!(confirmed, failure.written_bytes());
            operation
        })
        .await;
        let disarm = probe.disarm();
        if let Err(payload) = result {
            if let Err(error) = disarm {
                let primary = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("non-string assertion panic");
                panic!(
                    "write cancellation: stage assertion failed and gate disarm failed: {error}; assertion: {primary}"
                );
            }
            resume_unwind(payload);
        }
        disarm.expect("write cancellation: gate disarm failed");
        let mut operation = result.expect("successful assertion retained operation");
        let snapshot = operation.written_bytes();
        if let Some(writer) = operation.recovery_writer() {
            let outcome = writer
                .abort_async()
                .await
                .expect("write cancellation: writer recovery failed");
            let expected_state = match outcome {
                WriteAbortOutcome::NotPublished => WriterState::Aborted,
                WriteAbortOutcome::Published => WriterState::Published,
                WriteAbortOutcome::Indeterminate => WriterState::Indeterminate,
            };
            assert_eq!(expected_state, writer.state());
        }
        assert_eq!(snapshot, operation.written_bytes());
        assert_eq!(
            AsyncWriteAllOperationState::Failed(WriteFailureState::Indeterminate),
            operation.state()
        );
        self.write_check(check_id(stage), ContractCheckOutcome::Passed);
    }

    /// Records a check with its common write capability requirement.
    fn write_check(&mut self, id: &'static str, outcome: ContractCheckOutcome) {
        self.context
            .record_check(id, Some(FileSystemCapability::Write), outcome);
    }
}

/// Maps the provider stage to its stable evidence identifier.
const fn check_id(stage: AsyncWriteCancellationStage) -> &'static str {
    match stage {
        AsyncWriteCancellationStage::Open => "write/cancel-open",
        AsyncWriteCancellationStage::Write => "write/cancel-write",
        AsyncWriteCancellationStage::Flush => "write/cancel-flush",
        AsyncWriteCancellationStage::Commit => "write/cancel-commit",
    }
}
