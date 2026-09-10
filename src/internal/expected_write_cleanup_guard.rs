// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
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
