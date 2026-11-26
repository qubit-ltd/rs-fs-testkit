// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Returns an interrupted expected-copy rejection and operation to run evidence.

use crate::ContractAsyncCopyFailure;
use crate::ContractCheckId;
use crate::ContractFailure;

/// Retains the original copy failure and operation while explicit abort awaits.
pub(crate) struct CopyConflictCleanupGuard<'a> {
    failure: Option<ContractAsyncCopyFailure>,
    failures: &'a mut Vec<ContractFailure>,
    id: ContractCheckId,
}

impl<'a> CopyConflictCleanupGuard<'a> {
    /// Transfers a failure into the guard before abort can suspend.
    pub(crate) fn new(
        failure: ContractAsyncCopyFailure,
        failures: &'a mut Vec<ContractFailure>,
        id: ContractCheckId,
    ) -> Self {
        Self {
            failure: Some(failure),
            failures,
            id,
        }
    }

    /// Borrows the retained diagnostic and operation during abort.
    pub(crate) fn failure_mut(&mut self) -> &mut ContractAsyncCopyFailure {
        self.failure.as_mut().expect("copy cleanup guard retains its failure")
    }

    /// Returns ownership after abort resolves.
    pub(crate) fn finish(mut self) -> ContractAsyncCopyFailure {
        self.failure.take().expect("copy cleanup guard finishes once")
    }
}

impl Drop for CopyConflictCleanupGuard<'_> {
    /// Retains the original copy facts and still-owned operation without I/O.
    fn drop(&mut self) {
        if let Some(failure) = self.failure.take() {
            self.failures.push(
                ContractFailure::with_owned_source(
                    "copy conflict recovery was cancelled; original failure and operation retained",
                    failure,
                )
                .at(self.id),
            );
        }
    }
}
