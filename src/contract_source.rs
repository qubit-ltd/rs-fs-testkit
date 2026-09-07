// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
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
