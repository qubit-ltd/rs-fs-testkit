// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Copy cancellation evidence and explicit writer recovery.

use std::future::Future;
use std::future::poll_fn;
use std::task::Poll;

use qubit_fs::copy::AsyncCopyOperation;
use qubit_fs::copy::AsyncCopyOperationState;
use qubit_fs::copy::CopyFailureState;
use qubit_fs::error::FsErrorKind;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::path::Path;
use qubit_fs::write::AsyncWriterRecovery;
use qubit_fs::write::WriteAbortOutcome;
use qubit_fs::write::WriterState;

use crate::AsyncCopyCancellationStage;
use crate::AsyncFileSystemContractSuite;
use crate::AsyncFileSystemFixture;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::CopyCancellationProbe;
use crate::FixtureSupport;
use crate::internal::probe_disarm_guard::ProbeDisarmGuard;
use crate::internal::verify_condition;

impl AsyncFileSystemContractSuite<'_> {
    /// Executes only the selected stage and records no sibling outcomes.
    pub(super) async fn check_copy_cancellation_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        let stage = match id {
            ContractCheckId::AsyncCopyCancelNativeAttempt => AsyncCopyCancellationStage::NativeAttempt,
            ContractCheckId::AsyncCopyCancelReader => AsyncCopyCancellationStage::Reader,
            ContractCheckId::AsyncCopyCancelWriter => AsyncCopyCancellationStage::Writer,
            ContractCheckId::AsyncCopyCancelCommit => AsyncCopyCancellationStage::Commit,
            _ => {
                return Err(ContractFailure::message_only("selected check is not copy cancellation").at(id));
            }
        };
        self.context.begin(id.as_str());
        if !self.capable(FileSystemCapability::Copy) {
            self.context.record_check(
                id,
                Some(FileSystemCapability::Copy),
                ContractCheckOutcome::NotApplicable {
                    reason: "basic copy capability is unavailable".to_owned(),
                },
            );
            return Ok(());
        }
        let relative = self.context.relative_name(&format!("async-copy-cancel-{stage:?}"));
        let prepared = self
            .fixture
            .prepare_copy_cancellation(stage, &relative)
            .await
            .map_err(|error| ContractFailure::with_source("copy cancellation preparation failed", error).at(id))?;
        match prepared {
            FixtureSupport::Supported(probe) => self.run_cancellation_probe(stage, probe).await?,
            FixtureSupport::Unsupported => self.context.record_check(
                id,
                Some(FileSystemCapability::Copy),
                ContractCheckOutcome::SkippedOptional {
                    reason: "fixture has no stage acknowledgement probe".to_owned(),
                },
            ),
        }
        Ok(())
    }

    /// Stops execution before releasing the gate, then validates recovery
    /// facts.
    ///
    /// # Parameters
    ///
    /// * `stage` - Cancellation stage whose acknowledgement must be reached.
    /// * `probe` - Fixture probe that gates the selected copy stage.
    ///
    /// # Errors
    ///
    /// Returns a [`ContractFailure`] if the copy cannot be observed, cancelled,
    /// or recovered according to the stage contract.
    async fn run_cancellation_probe(
        &mut self,
        stage: AsyncCopyCancellationStage,
        probe: Box<dyn CopyCancellationProbe>,
    ) -> Result<(), ContractFailure> {
        let id = cancellation_check_id(stage);
        let (source, target, options) = probe.case().clone().into_parts();
        self.context.record_created(source.clone());
        self.context.record_created(target.clone());
        let mut guard = ProbeDisarmGuard::new(|| probe.disarm(), &mut self.context.run.cleanup.failures);
        let source_before = observe_file(self.fixture, &source, id)
            .await?
            .ok_or_else(|| ContractFailure::message_only("copy cancellation source is absent").at(id))?;
        let target_before = observe_file(self.fixture, &target, id).await?;
        let mut operation = self
            .fixture
            .file_system()
            .begin_copy(source.clone(), target.clone(), options)
            .map_err(|error| {
                ContractFailure::with_owned_source(
                    "copy cancellation request admission failed",
                    crate::ContractAsyncCopyFailure::new(error, None),
                )
                .at(id)
            })?;
        drop(operation.execute());
        verify_condition(
            operation.state() == AsyncCopyOperationState::Ready,
            id,
            "unpolled copy execution changed state",
        )?;
        verify_condition(!operation.has_recovery(), id, "unpolled copy acquired a writer")?;
        let mut execution_failure = None;
        let mut execution = Box::pin(operation.execute());
        let reached = poll_fn(|context| match execution.as_mut().poll(context) {
            Poll::Pending => probe.poll_reached(context).map(|result| {
                result.map_err(|error| ContractFailure::with_source("copy stage acknowledgement failed", error).at(id))
            }),
            Poll::Ready(Ok(_)) => Poll::Ready(Err(ContractFailure::message_only(
                "copy completed before requested stage acknowledgement",
            )
            .at(id))),
            Poll::Ready(Err(error)) => {
                execution_failure = Some(error);
                Poll::Ready(Err(ContractFailure::message_only(
                    "copy failed before requested stage acknowledgement",
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
                "copy failed before requested stage acknowledgement",
                crate::ContractAsyncCopyFailure::new(error, Some(operation)),
            )
            .at(id));
        }
        if let Err(error) = reached {
            return Err(retain_copy_operation(error, operation, id));
        }
        if let Err(error) = verify_condition(
            disarmed,
            id,
            "probe disarm failed; original cause is retained in cleanup",
        ) {
            return Err(retain_copy_operation(error, operation, id));
        }
        if let Err(error) = verify_condition(
            operation.state() == AsyncCopyOperationState::Failed(CopyFailureState::Indeterminate),
            id,
            "cancelled copy did not become indeterminate",
        ) {
            return Err(retain_copy_operation(error, operation, id));
        }
        let has_writer = matches!(
            stage,
            AsyncCopyCancellationStage::Writer | AsyncCopyCancellationStage::Commit
        );
        if let Err(error) = verify_condition(
            operation.has_recovery() == has_writer,
            id,
            "copy recovery writer responsibility differs",
        ) {
            return Err(retain_copy_operation(error, operation, id));
        }
        let repeated = match operation.execute().await {
            Err(error) => error,
            Ok(_) => {
                return Err(retain_copy_operation(
                    ContractFailure::message_only("cancelled copy executed twice").at(id),
                    operation,
                    id,
                ));
            }
        };
        if repeated.error().kind() != FsErrorKind::InvalidState || repeated.state() != CopyFailureState::Indeterminate {
            return Err(ContractFailure::with_owned_source(
                "repeated copy lost cancellation facts",
                crate::ContractAsyncCopyFailure::new(repeated, Some(operation)),
            )
            .at(id));
        }
        if let Err(error) = verify_condition(
            repeated.partial_stats().bytes <= source_before.len() as u64,
            id,
            "copy progress exceeds independently observed source bytes",
        ) {
            return Err(retain_copy_operation(error, operation, id));
        }
        let repeated_again = match operation.execute().await {
            Err(error) => error,
            Ok(_) => {
                return Err(retain_copy_operation(
                    ContractFailure::message_only("cancelled copy executed a third time").at(id),
                    operation,
                    id,
                ));
            }
        };
        if repeated_again.error().kind() != FsErrorKind::InvalidState
            || repeated_again.state() != repeated.state()
            || repeated_again.partial_stats() != repeated.partial_stats()
        {
            return Err(ContractFailure::with_owned_source(
                "repeated copy changed retained recovery facts",
                crate::ContractAsyncCopyFailure::new(repeated_again, Some(operation)),
            )
            .at(id));
        }
        if let Err(error) = verify_condition(
            operation.state() == AsyncCopyOperationState::Failed(CopyFailureState::Indeterminate),
            id,
            "repeat changed cancelled copy state",
        ) {
            return Err(retain_copy_operation(error, operation, id));
        }
        if let Err(error) = verify_condition(
            operation.has_recovery() == has_writer,
            id,
            "repeat lost copy recovery writer",
        ) {
            return Err(retain_copy_operation(error, operation, id));
        }
        if let Some(recovery) = operation.take_recovery() {
            let mut writer = match recovery {
                AsyncWriterRecovery::Opened(writer) => *writer,
                AsyncWriterRecovery::Rejected(recovery) => {
                    return Err(ContractFailure::with_owned_source(
                        "cancellation probe retained a rejected provider identity",
                        crate::ContractWriterFailure::new(
                            ContractFailure::message_only("validated writer required by this stage probe"),
                            recovery,
                        ),
                    )
                    .at(id));
                }
            };
            let outcome = match writer.abort_async().await {
                Ok(outcome) => outcome,
                Err(error) => {
                    return Err(ContractFailure::with_owned_source(
                        "copy recovery abort failed",
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
            verify_condition(
                writer.state() == expected,
                id,
                "copy recovery state differs from abort outcome",
            )?;
            let target_after = observe_file(self.fixture, &target, id).await?;
            match outcome {
                WriteAbortOutcome::NotPublished => verify_condition(
                    target_after == target_before,
                    id,
                    "NotPublished copy recovery changed target",
                )?,
                WriteAbortOutcome::Published => verify_condition(
                    target_after.as_deref() == Some(source_before.as_slice()),
                    id,
                    "published copy recovery differs from source",
                )?,
                WriteAbortOutcome::Indeterminate => {}
            }
        }
        let source_after = observe_file(self.fixture, &source, id).await?;
        verify_condition(
            source_after.as_deref() == Some(source_before.as_slice()),
            id,
            "cancelled copy changed source",
        )?;
        self.context
            .record_check(id, Some(FileSystemCapability::Copy), ContractCheckOutcome::Passed);
        Ok(())
    }
}

fn retain_copy_operation(
    failure: ContractFailure,
    operation: AsyncCopyOperation,
    id: ContractCheckId,
) -> ContractFailure {
    ContractFailure::with_owned_source(
        "copy cancellation diagnostic failed; operation retained",
        crate::ContractWriterFailure::new(failure, operation),
    )
    .at(id)
}

/// Returns the stable identity for one cancellation stage.
#[inline]
const fn cancellation_check_id(stage: AsyncCopyCancellationStage) -> ContractCheckId {
    match stage {
        AsyncCopyCancellationStage::NativeAttempt => ContractCheckId::AsyncCopyCancelNativeAttempt,
        AsyncCopyCancellationStage::Reader => ContractCheckId::AsyncCopyCancelReader,
        AsyncCopyCancellationStage::Writer => ContractCheckId::AsyncCopyCancelWriter,
        AsyncCopyCancellationStage::Commit => ContractCheckId::AsyncCopyCancelCommit,
    }
}

/// Requires independent absence or complete file-content evidence.
///
/// # Parameters
///
/// * `fixture` - Fixture providing independent filesystem observations.
/// * `path` - File path whose existence and contents are observed.
/// * `id` - Contract check identity attached to observation failures.
///
/// # Returns
///
/// `Ok(None)` when the path is independently confirmed absent, or `Ok(Some)`
/// with its complete contents when present.
///
/// # Errors
///
/// Returns a [`ContractFailure`] when an observation fails or the fixture does
/// not support the evidence required for the path.
async fn observe_file(
    fixture: &dyn AsyncFileSystemFixture,
    path: &Path,
    id: ContractCheckId,
) -> Result<Option<Vec<u8>>, ContractFailure> {
    match fixture
        .exists_out_of_band(path)
        .await
        .map_err(|error| ContractFailure::with_source("copy independent existence observation failed", error).at(id))?
    {
        FixtureSupport::Supported(false) => return Ok(None),
        FixtureSupport::Supported(true) => {}
        FixtureSupport::Unsupported => {
            return Err(ContractFailure::message_only("copy probe has no independent existence evidence").at(id));
        }
    }
    match fixture
        .read_file(path)
        .await
        .map_err(|error| ContractFailure::with_source("copy independent byte observation failed", error).at(id))?
    {
        FixtureSupport::Supported(bytes) => Ok(Some(bytes)),
        FixtureSupport::Unsupported => {
            Err(ContractFailure::message_only("copy probe has no independent byte evidence").at(id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AsyncFileSystemContractSuite;
    use crate::ContractCheckId;
    use crate::common::AsyncMemoryFixture;
    use crate::common::async_memory_file_system::run_controlled;

    #[test]
    fn test_rejects_a_non_cancellation_check_identity() {
        let fixture = AsyncMemoryFixture::new();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let result = suite.check_copy_cancellation_item(ContractCheckId::ReadBasic).await;
            assert!(result.is_err(), "non-cancellation check identity was accepted");
        });
    }
}
