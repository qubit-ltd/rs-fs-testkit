// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Explicit cleanup before accepting an expected whole-file write rejection.

use qubit_fs::write::WriteAbortOutcome;
use qubit_fs::write::WriteAllFailure;
use qubit_fs::write::WriteFailureState;
use qubit_fs::write::WriterRecovery;

use crate::ContractCheckId;
use crate::ContractFailure;
use crate::ContractWriterFailure;

/// Confirms non-publication and cleans a retained session before accepting it.
///
/// Any unexpected state or unsuccessful cleanup retains the original snapshot
/// and session in the returned contract failure. No publication is retried.
pub(crate) fn finish_expected_write_failure(
    mut failure: WriteAllFailure,
    id: ContractCheckId,
) -> Result<(), ContractFailure> {
    // Provider-open failures have no returned session to clean; their original
    // effect evidence stays conservative and the caller checks target contents.
    if failure.recovery().is_none() {
        return Ok(());
    }
    if !matches!(
        failure.state(),
        WriteFailureState::NotPublished | WriteFailureState::RetryableNotPublished
    ) {
        return Err(
            ContractFailure::with_owned_source("write rejection did not prove non-publication", failure).at(id),
        );
    }
    let result = match failure.recovery_mut() {
        None => return Ok(()),
        Some(WriterRecovery::Opened(writer)) => writer.abort(),
        Some(WriterRecovery::Rejected(_)) => {
            return Err(
                ContractFailure::with_owned_source("write rejection retained an invalid session", failure).at(id),
            );
        }
    };
    match result {
        Ok(WriteAbortOutcome::NotPublished) => Ok(()),
        Ok(_) => Err(ContractFailure::with_owned_source(
            "write rejection cleanup did not confirm non-publication",
            failure,
        )
        .at(id)),
        Err(error) => Err(ContractFailure::with_owned_source(
            "expected write rejection cleanup failed",
            ContractWriterFailure::new(error, failure),
        )
        .at(id)),
    }
}

/// Applies the same ownership rule to an admitted asynchronous operation.
///
/// Cleanup borrows the session retained in the failure; cleanup errors preserve
/// both that operation and its immutable execution failure snapshot.
#[cfg(feature = "async")]
pub(crate) async fn finish_expected_async_write_failure(
    failure: crate::ContractAsyncWriteFailure,
    id: ContractCheckId,
    failures: &mut Vec<ContractFailure>,
) -> Result<(), ContractFailure> {
    use qubit_fs::write::AsyncWriterRecovery;

    use crate::internal::expected_write_cleanup_guard::ExpectedWriteCleanupGuard;

    if failure.operation().is_none_or(|operation| !operation.has_recovery()) {
        return Ok(());
    }

    if !matches!(
        failure.failure().state(),
        WriteFailureState::NotPublished | WriteFailureState::RetryableNotPublished
    ) {
        return Err(
            ContractFailure::with_owned_source("write rejection did not prove non-publication", failure).at(id),
        );
    }
    let mut guard = ExpectedWriteCleanupGuard::new(failure, failures, id);
    let result = match guard
        .failure_mut()
        .operation_mut()
        .and_then(|operation| operation.recovery())
    {
        None => {
            drop(guard.finish());
            return Ok(());
        }
        Some(AsyncWriterRecovery::Opened(writer)) => writer.abort_async().await,
        Some(AsyncWriterRecovery::Rejected(_)) => {
            return Err(ContractFailure::with_owned_source(
                "write rejection retained an invalid session",
                guard.finish(),
            )
            .at(id));
        }
    };
    let failure = guard.finish();
    match result {
        Ok(WriteAbortOutcome::NotPublished) => Ok(()),
        Ok(_) => Err(ContractFailure::with_owned_source(
            "write rejection cleanup did not confirm non-publication",
            failure,
        )
        .at(id)),
        Err(error) => Err(ContractFailure::with_owned_source(
            "expected write rejection cleanup failed",
            ContractWriterFailure::new(error, failure),
        )
        .at(id)),
    }
}
