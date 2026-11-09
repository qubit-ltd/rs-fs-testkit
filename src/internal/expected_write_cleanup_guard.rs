// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Returns cleanup ownership to the run when an expected-rejection check is
//! cancelled.

use crate::ContractAsyncWriteFailure;
use crate::ContractCheckId;
use crate::ContractFailure;

/// Saves the original rejection if its explicit cleanup future is abandoned.
pub(crate) struct ExpectedWriteCleanupGuard<'a> {
    /// Failure whose operation continues owning the cleanup session.
    failure: Option<ContractAsyncWriteFailure>,
    /// Report storage outside the cancellation domain.
    failures: &'a mut Vec<ContractFailure>,
    /// Check responsible for this cleanup attempt.
    id: ContractCheckId,
}

impl<'a> ExpectedWriteCleanupGuard<'a> {
    /// Transfers the failure into a guard before polling explicit cleanup.
    ///
    /// # Parameters
    ///
    /// * `failure` — the original operation failure whose recovery state is
    ///   being retained.
    /// * `failures` — the collection that receives cleanup failures.
    /// * `id` — the stable identity of the contract check being executed or
    ///   recorded.
    pub(crate) fn new(
        failure: ContractAsyncWriteFailure,
        failures: &'a mut Vec<ContractFailure>,
        id: ContractCheckId,
    ) -> Self {
        Self {
            failure: Some(failure),
            failures,
            id,
        }
    }

    /// Borrows the operation while its cleanup future is active.
    #[inline]
    pub(crate) fn failure_mut(&mut self) -> &mut ContractAsyncWriteFailure {
        self.failure
            .as_mut()
            .expect("cleanup guard owns its failure until finish")
    }

    /// Returns ownership after cleanup resolves, disabling cancellation
    /// reporting.
    pub(crate) fn finish(mut self) -> ContractAsyncWriteFailure {
        self.failure.take().expect("cleanup guard finishes once")
    }
}

impl Drop for ExpectedWriteCleanupGuard<'_> {
    /// Records ownership synchronously without invoking provider cleanup.
    fn drop(&mut self) {
        if let Some(failure) = self.failure.take() {
            self.failures.push(
                ContractFailure::with_owned_source(
                    "expected write rejection cleanup was cancelled; original failure retained",
                    failure,
                )
                .at(self.id),
            );
        }
    }
}

#[cfg(all(test, feature = "async"))]
mod tests {
    use std::future::Future;
    use std::task::Context;
    use std::task::Poll;
    use std::task::Waker;

    use qubit_fs::write::WriteOptions;

    use crate::ContractAsyncWriteFailure;
    use crate::ContractCheckId;
    use crate::internal::expected_write_cleanup_guard::ExpectedWriteCleanupGuard;
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

    fn retained_failure() -> ContractAsyncWriteFailure {
        let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::OwningCommitFails);
        let path = fixture.path("owning-write").expect("fixture path");
        let mut operation = fixture
            .file_system()
            .begin_write_all(path, b"payload".to_vec(), WriteOptions::default())
            .expect("write operation should be admitted");
        let failure = ready(operation.execute()).expect_err("commit should fail");
        assert!(operation.has_recovery());
        ContractAsyncWriteFailure::new(failure, Some(operation))
    }

    #[test]
    fn test_finishing_guard_returns_failure_without_reporting_cancellation() {
        let mut failures = Vec::new();
        let failure = retained_failure();
        let guard = ExpectedWriteCleanupGuard::new(failure, &mut failures, ContractCheckId::WriteIfAbsent);

        let retained = guard.finish();

        assert!(retained.operation().is_some());
        assert!(failures.is_empty());
    }

    #[test]
    fn test_dropping_guard_records_check_and_retains_original_failure() {
        let mut failures = Vec::new();
        let failure = retained_failure();
        {
            let mut guard = ExpectedWriteCleanupGuard::new(failure, &mut failures, ContractCheckId::WriteIfMatch);
            assert!(guard.failure_mut().operation().is_some());
        }

        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].check(), Some(ContractCheckId::WriteIfMatch));
        assert!(failures[0].message().contains("cleanup was cancelled"));
        assert!(std::error::Error::source(&failures[0]).is_some());
    }
}
