// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Original resource errors paired with their still-owned recovery session.

use std::error::Error;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;

/// A failed temporary lifecycle operation whose resource remains available for
/// recovery.
///
/// # Type Parameters
///
/// - `T`: The synchronous or asynchronous temporary resource whose recovery
///   remains the caller's responsibility after it is taken.
///
/// A temporary resource check in a contract suite can report this value when a
/// provider operation fails while the resource is still available. Downcast
/// the failure's source to this type to recover the resource. Taking it
/// transfers recovery responsibility to the caller. Formatting does not
/// inspect the resource or invoke the provider's error formatter.
///
/// # Examples
///
/// ```
/// use std::error::Error;
///
/// use qubit_fs::temp::TempFile;
/// use qubit_fs_testkit::ContractRun;
/// use qubit_fs_testkit::ContractSource;
/// use qubit_fs_testkit::ContractTempFailure;
///
/// fn take_temp_failure(
///     run: &ContractRun,
/// ) -> Result<Option<Box<ContractTempFailure<TempFile>>>, Box<dyn Error + Send>> {
///     for failure in run.failures() {
///         let Some(source) = Error::source(failure)
///             .and_then(|source| source.downcast_ref::<ContractSource>())
///         else {
///             continue;
///         };
///         let Some(owned) = source.take() else {
///             continue;
///         };
///         return owned.downcast::<ContractTempFailure<TempFile>>().map(Some);
///     }
///     Ok(None)
/// }
/// ```
///
/// If downcasting returns `Err`, it returns the owned source to the caller;
/// do not discard it because it may carry recovery responsibility. Each source
/// can be taken only once, and taking it does not remove the historical
/// failure from the `ContractRun`.
#[must_use]
pub struct ContractTempFailure<T> {
    /// Preserves the original operation error for inspection and chaining.
    error: Box<dyn Error + Send>,
    /// Keeps the temporary resource owned until the caller recovers or drops
    /// it.
    resource: T,
}

impl<T> ContractTempFailure<T> {
    /// Retains a temporary resource failure before its resource can leave
    /// scope.
    ///
    /// # Parameters
    ///
    /// - `error`: The original operation error to preserve for later
    ///   inspection.
    /// - `resource`: The temporary resource whose recovery responsibility is
    ///   being retained.
    ///
    /// # Returns
    ///
    /// A failure value that owns the error and resource until the caller takes
    /// or drops them.
    pub(crate) fn new(error: impl Error + Send + 'static, resource: T) -> Self {
        Self {
            error: Box::new(error),
            resource,
        }
    }

    /// Returns the original error for typed inspection.
    ///
    /// # Returns
    ///
    /// A borrow of the original error, which can be downcast without consuming
    /// the retained recovery resource.
    #[must_use = "inspect or retain the original error"]
    #[inline]
    pub fn error(&self) -> &(dyn Error + Send + 'static) {
        self.error.as_ref()
    }

    /// Borrows the retained resource for lifecycle inspection.
    ///
    /// # Returns
    ///
    /// An immutable borrow of the retained resource, valid for the lifetime of
    /// the borrow of this failure.
    #[must_use = "inspect the retained resource"]
    #[inline]
    pub const fn resource(&self) -> &T {
        &self.resource
    }

    /// Borrows the retained resource for explicit recovery or cleanup.
    ///
    /// # Returns
    ///
    /// A mutable borrow of the retained resource, allowing the caller to
    /// perform recovery or cleanup before transferring or dropping it.
    #[must_use = "use the retained resource to recover or clean it up"]
    #[inline]
    pub fn resource_mut(&mut self) -> &mut T {
        &mut self.resource
    }

    /// Transfers both the original error and its resource to the caller.
    ///
    /// # Returns
    ///
    /// The original error and retained resource, transferring ownership of both
    /// to the caller.
    #[must_use = "handle the transferred error and resource"]
    pub fn into_parts(self) -> (Box<dyn Error + Send>, T) {
        (self.error, self.resource)
    }
}

impl<T> Debug for ContractTempFailure<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("ContractTempFailure { resource: retained, error: retained }")
    }
}

impl<T> Display for ContractTempFailure<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("temporary lifecycle failed; original error and recovery resource retained")
    }
}

impl<T> Error for ContractTempFailure<T> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.error.as_ref())
    }
}
