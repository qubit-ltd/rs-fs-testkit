// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Explicit cleanup before accepting an expected whole-file write rejection.

use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;

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
///
/// # Parameters
///
/// * `failure` — the original operation failure whose recovery state is being
///   retained.
/// * `id` — the stable identity of the contract check being executed or
///   recorded.
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
    let writer = match failure.recovery_mut() {
        None => return Ok(()),
        Some(WriterRecovery::Opened(writer)) => writer,
        Some(WriterRecovery::Rejected(_)) => {
            return Err(
                ContractFailure::with_owned_source("write rejection retained an invalid session", failure).at(id),
            );
        }
    };
    let result = match catch_unwind(AssertUnwindSafe(|| writer.abort())) {
        Ok(result) => result,
        Err(payload) => {
            return Err(ContractFailure::with_owned_source(
                "expected write rejection cleanup panicked",
                ContractWriterFailure::new(ContractFailure::panicked("writer abort panicked", payload), failure),
            )
            .at(id));
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
///
/// # Parameters
///
/// * `failure` — the original operation failure whose recovery state is being
///   retained.
/// * `id` — the stable identity of the contract check being executed or
///   recorded.
/// * `failures` — the collection that receives cleanup failures.
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

#[cfg(test)]
mod tests {
    use qubit_fs::write::WriteOptions;

    use super::finish_expected_write_failure;
    use crate::ContractCheckId;
    use crate::FileSystemFixture;
    use crate::tests::common::MemoryFault;
    use crate::tests::common::MemoryFixture;

    #[test]
    fn test_sync_rejection_cleanup_completes_without_retaining_a_contract_failure() {
        let fixture = MemoryFixture::with_fault(MemoryFault::WriteCommitFailure);
        let path = fixture.path("expected-rejection").expect("fixture path");
        let failure = fixture
            .file_system()
            .write_all(&path, b"payload", WriteOptions::default())
            .expect_err("commit should fail");

        let result = finish_expected_write_failure(failure, ContractCheckId::WriteIfAbsent);

        assert!(result.is_ok());
    }

    #[cfg(feature = "async")]
    #[test]
    fn test_async_rejection_cleanup_completes_without_reporting_failure() {
        use std::future::Future;
        use std::task::Context;
        use std::task::Poll;
        use std::task::Waker;

        use qubit_fs::write::AsyncWriteAllOperation;
        use qubit_fs::write::WriteOptions;

        use super::finish_expected_async_write_failure;
        use crate::ContractAsyncWriteFailure;
        use crate::qubit_fs_testkit::AsyncFileSystemFixture;
        use crate::tests::common::AsyncMemoryFault;
        use crate::tests::common::AsyncMemoryFixture;

        fn ready<F: Future>(future: F) -> F::Output {
            let mut future = std::pin::pin!(future);
            let mut context = Context::from_waker(Waker::noop());
            match future.as_mut().poll(&mut context) {
                Poll::Ready(output) => output,
                Poll::Pending => panic!("memory write fixture must complete immediately"),
            }
        }

        let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::OwningCommitFails);
        let path = fixture.path("owning-write").expect("fixture path");
        let mut operation: AsyncWriteAllOperation = fixture
            .file_system()
            .begin_write_all(path, b"payload".to_vec(), WriteOptions::default())
            .expect("write operation should be admitted");
        let failure = ready(operation.execute()).expect_err("commit should fail");
        let failure = ContractAsyncWriteFailure::new(failure, Some(operation));
        let mut reported = Vec::new();

        let result = ready(finish_expected_async_write_failure(
            failure,
            ContractCheckId::WriteIfAbsent,
            &mut reported,
        ));

        assert!(result.is_ok());
        assert!(reported.is_empty());
    }
}
