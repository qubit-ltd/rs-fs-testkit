// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Owned provider failures that may retain a non-Sync recovery handle.

use std::error::Error;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::sync::Mutex;

/// Preserves the complete original error while keeping the report Send + Sync.
///
/// Some filesystem errors own a recovery writer that is Send but not Sync.
/// This wrapper serializes explicit inspection and ownership transfer.
/// Formatting never invokes provider code. Taking the source transfers any
/// recovery responsibility; otherwise ordinary destruction still drops it.
///
/// # Examples
///
/// A run returned by `FileSystemContractSuite::run` exposes failures through
/// `ContractRun::failures`. A failed owned write may retain this wrapper in the
/// failure's standard error source chain. Taking the source transfers the
/// provider error and any recovery responsibility:
///
/// ```
/// use std::error::Error;
///
/// use qubit_fs_testkit::ContractRun;
/// use qubit_fs_testkit::ContractSource;
///
/// fn take_owned_source(run: &ContractRun) -> Option<Box<dyn Error + Send>> {
///     let failure = run.failures().first()?;
///     Error::source(failure)?
///         .downcast_ref::<ContractSource>()?
///         .take()
/// }
/// ```
#[must_use]
pub struct ContractSource {
    /// Serializes access to the retained error and allows recovery after
    /// poisoning.
    error: Mutex<Option<Box<dyn Error + Send>>>,
}

impl ContractSource {
    /// Retains an original typed error without formatting provider internals.
    pub(crate) fn new(error: impl Error + Send + 'static) -> Self {
        Self {
            error: Mutex::new(Some(Box::new(error))),
        }
    }

    /// Inspects the original concrete error while it remains owned by the run.
    ///
    /// # Returns
    ///
    /// Returns `Some` with the result of `inspect` while the source is
    /// retained. Returns `None` after a caller has taken ownership of the
    /// source.
    pub fn inspect<T>(&self, inspect: impl FnOnce(&(dyn Error + Send + 'static)) -> T) -> Option<T> {
        let guard = self.error.lock().unwrap_or_else(|poison| poison.into_inner());
        guard.as_deref().map(inspect)
    }

    /// Transfers the original error and any recovery handle to the caller.
    ///
    /// The caller becomes responsible for its explicit recovery or disposal.
    ///
    /// # Returns
    ///
    /// The original error if it is still retained, or `None` if another caller
    /// has already taken it.
    pub fn take(&self) -> Option<Box<dyn Error + Send>> {
        self.error.lock().unwrap_or_else(|poison| poison.into_inner()).take()
    }
}

impl Debug for ContractSource {
    /// Formats a stable summary without invoking the retained provider error.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("ContractSource { original_error: retained }")
    }
}

impl Display for ContractSource {
    /// Formats recovery guidance without exposing provider-specific details.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("original provider failure; inspect the retained source for recovery")
    }
}

impl Error for ContractSource {}

#[cfg(test)]
mod tests {
    use std::panic::AssertUnwindSafe;

    use super::ContractSource;

    #[test]
    fn test_poisoned_source_lock_remains_inspectable_and_transferable() {
        let source = ContractSource::new(std::io::Error::other("poisoned source"));
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = source.error.lock().unwrap();
            panic!("poison source lock");
        }));

        assert_eq!(
            source.inspect(|error| error.to_string()),
            Some("poisoned source".to_owned())
        );
        assert_eq!(source.inspect(|error| error.is::<std::io::Error>()), Some(true));
        assert_eq!(source.take().unwrap().to_string(), "poisoned source");
        assert!(source.take().is_none());
    }
}
