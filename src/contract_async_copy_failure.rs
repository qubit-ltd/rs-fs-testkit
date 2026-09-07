// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Copy operation failures paired with their recovery operation.

use std::error::Error;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;

use qubit_fs::copy::AsyncCopyFailure;
use qubit_fs::copy::AsyncCopyOperation;
use qubit_fs::error::FsError;

/// Retains a copy failure snapshot and its operation, when admitted.
///
/// Obtain this value through [`crate::ContractSource::take`] and downcast it.
/// Admission failures have no operation. Execution failures retain the
/// operation even when it has no recovery writer. Taking this value transfers
/// any recovery responsibility to the caller; dropping it does not perform
/// asynchronous abort. Formatting never invokes a provider formatter.
#[must_use]
pub struct ContractAsyncCopyFailure {
    failure: AsyncCopyFailure,
    operation: Option<AsyncCopyOperation>,
}

impl ContractAsyncCopyFailure {
    /// Returns the original filesystem error.
    pub const fn error(&self) -> &FsError {
        self.failure.error()
    }

    /// Returns the failure's publication state and partial transfer statistics
    /// snapshot.
    pub const fn failure(&self) -> &AsyncCopyFailure {
        &self.failure
    }

    /// Borrows the operation if request admission succeeded.
    pub const fn operation(&self) -> Option<&AsyncCopyOperation> {
        self.operation.as_ref()
    }

    /// Borrows the operation for explicit recovery writer transfer or
    /// inspection.
    pub fn operation_mut(&mut self) -> Option<&mut AsyncCopyOperation> {
        self.operation.as_mut()
    }

    /// Transfers the failure snapshot and optional operation to the caller.
    pub fn into_parts(self) -> (AsyncCopyFailure, Option<AsyncCopyOperation>) {
        (self.failure, self.operation)
    }

    /// Pairs a failure with the operation that owns its recovery session.
    pub(crate) fn new(failure: AsyncCopyFailure, operation: Option<AsyncCopyOperation>) -> Self {
        Self { failure, operation }
    }
}

impl Debug for ContractAsyncCopyFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter
            .debug_struct("ContractAsyncCopyFailure")
            .field("has_operation", &self.operation.is_some())
            .finish_non_exhaustive()
    }
}

impl Display for ContractAsyncCopyFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("copy operation failed; original failure and available operation retained")
    }
}

impl Error for ContractAsyncCopyFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.failure)
    }
}
