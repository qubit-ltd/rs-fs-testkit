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
/// After downcasting a failure chain to `ContractSource`, transfer ownership of
/// the retained provider error:
///
/// ```
/// use qubit_fs_testkit::ContractSource;
///
/// fn transfer(source: &ContractSource) -> Option<Box<dyn std::error::Error + Send>> {
///     source.take()
/// }
/// ```
#[must_use]
pub struct ContractSource {
    error: Mutex<Option<Box<dyn Error + Send>>>,
}

impl ContractSource {
    /// Inspects the original concrete error while it remains owned by the run.
    ///
    /// Returns None after a caller has taken ownership of the source.
    pub fn inspect<T>(&self, inspect: impl FnOnce(&(dyn Error + Send + 'static)) -> T) -> Option<T> {
        let guard = self.error.lock().unwrap_or_else(|poison| poison.into_inner());
        guard.as_deref().map(inspect)
    }

    /// Transfers the original error and any recovery handle to the caller.
    ///
    /// The caller becomes responsible for its explicit recovery or disposal.
    pub fn take(&self) -> Option<Box<dyn Error + Send>> {
        self.error.lock().unwrap_or_else(|poison| poison.into_inner()).take()
    }

    /// Retains an original typed error without formatting provider internals.
    pub(crate) fn new(error: impl Error + Send + 'static) -> Self {
        Self {
            error: Mutex::new(Some(Box::new(error))),
        }
    }
}

impl Debug for ContractSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("ContractSource { original_error: retained }")
    }
}

impl Display for ContractSource {
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
    fn source_inspection_and_transfer_preserve_original_error() {
        let source = ContractSource::new(std::io::Error::other("source"));
        assert_eq!(source.inspect(|error| error.to_string()), Some("source".to_owned()));
        assert!(format!("{source:?}").contains("retained"));
        assert!(format!("{source}").contains("inspect"));
        assert_eq!(source.take().expect("source must transfer").to_string(), "source");
        assert!(source.inspect(|_| ()).is_none());
        assert!(source.take().is_none());
    }

    #[test]
    fn poisoned_source_lock_remains_inspectable_and_transferable() {
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

    #[test]
    fn formatting_keeps_the_original_error_available_for_inspection() {
        let source = ContractSource::new(std::io::Error::other("private provider detail"));
        assert_eq!(
            format!("{source}"),
            "original provider failure; inspect the retained source for recovery"
        );
        assert!(format!("{source:?}").contains("original_error: retained"));
        assert_eq!(
            source.inspect(|error| error.to_string()),
            Some("private provider detail".to_owned())
        );
        assert_eq!(source.take().unwrap().to_string(), "private provider detail");
        assert!(source.inspect(|error| error.to_string()).is_none());
        assert!(source.inspect(|error| error.to_string().len()).is_none());
    }
}
