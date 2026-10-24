// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Preserved diagnostics from contract execution and cleanup.

use std::any::Any;
use std::error::Error;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::sync::Mutex;

use crate::ContractCheckId;

/// A failure with safe context and its original typed cause.
///
/// Formatting never invokes the provider's arbitrary `Debug` implementation.
/// A panic payload remains available for explicit caller inspection.
///
/// # Examples
///
/// ```
/// # mod support { include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/common/rustdoc_support.rs")); }
/// # use support::RustdocSyncFixture;
/// use qubit_fs_testkit::{ContractFailure, FileSystemContract, FileSystemContractSuite};
///
/// let fixture = RustdocSyncFixture::new();
/// let mut suite = FileSystemContractSuite::new(&fixture);
/// suite.run_contract(FileSystemContract::Properties);
/// suite.run_contract(FileSystemContract::Stat);
/// let report = suite.run();
/// assert!(!report.failures().is_empty());
/// let failure: &ContractFailure = &report.failures()[0];
/// assert!(failure.message().contains("session already started"));
/// ```
#[must_use]
pub struct ContractFailure {
    /// The execution entry associated with this failure, when known.
    check: Option<ContractCheckId>,
    /// Safe context that can be displayed without formatting the cause.
    message: String,
    /// The original typed error, retained for standard error-chain inspection.
    source: Option<Box<dyn Error + Send + Sync>>,
    /// The original panic payload, available for one-time ownership transfer.
    panic: Mutex<Option<Box<dyn Any + Send>>>,
}

impl ContractFailure {
    /// Returns the check identity when execution could attribute the failure.
    ///
    /// # Returns
    ///
    /// The execution check identity, or `None` when no check was associated
    /// with the failure.
    #[inline]
    pub const fn check(&self) -> Option<ContractCheckId> {
        self.check
    }

    /// Returns the diagnostic context without formatting its underlying cause.
    ///
    /// # Returns
    ///
    /// The safe diagnostic context stored with this failure.
    #[must_use]
    #[inline]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Takes the original panic payload, preserving its concrete type.
    ///
    /// This transfers ownership through a borrowed run. Returns `None` when
    /// the failure was not a panic or another caller already took its payload.
    /// Taking the payload does not remove the failure from the run's history.
    ///
    /// # Returns
    ///
    /// The original panic payload when one is retained and has not already
    /// been taken; otherwise, `None`.
    pub fn take_panic_payload(&self) -> Option<Box<dyn Any + Send>> {
        self.panic.lock().unwrap_or_else(|poison| poison.into_inner()).take()
    }

    /// Constructs a failure without an underlying provider cause.
    pub(crate) fn message_only(message: impl Into<String>) -> Self {
        Self {
            check: None,
            message: message.into(),
            source: None,
            panic: Mutex::new(None),
        }
    }

    /// Retains the typed source separately from safe diagnostic context.
    pub(crate) fn with_source(message: impl Into<String>, source: impl Error + Send + Sync + 'static) -> Self {
        Self {
            check: None,
            message: message.into(),
            source: Some(Box::new(source)),
            panic: Mutex::new(None),
        }
    }

    /// Retains errors that own a non-Sync recovery handle without discarding
    /// it.
    pub(crate) fn with_owned_source(message: impl Into<String>, source: impl Error + Send + 'static) -> Self {
        Self::with_source(message, crate::ContractSource::new(source))
    }

    /// Preserves an unexpected panic at the external execution boundary.
    pub(crate) fn panicked(context: &str, payload: Box<dyn Any + Send>) -> Self {
        let detail = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied());
        let message = match detail {
            Some(detail) => format!("{context}: {detail}"),
            None => format!("{context}: non-string panic payload retained"),
        };
        Self {
            check: None,
            message,
            source: None,
            panic: Mutex::new(Some(payload)),
        }
    }

    /// Attributes a preserved failure to its typed execution entry.
    pub(crate) fn at(mut self, check: ContractCheckId) -> Self {
        self.check = Some(check);
        self
    }
}

impl Debug for ContractFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter
            .debug_struct("ContractFailure")
            .field("check", &self.check)
            .field("message", &self.message)
            .finish_non_exhaustive()
    }
}

impl Display for ContractFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        if let Some(check) = self.check {
            write!(formatter, "{check}: ")?;
        }
        formatter.write_str(&self.message)
    }
}

impl Error for ContractFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|source| source as &(dyn Error + 'static))
    }
}

#[cfg(test)]
mod tests {
    use std::panic::AssertUnwindSafe;

    use super::ContractFailure;

    #[test]
    fn test_poisoned_panic_lock_still_allows_payload_transfer() {
        let failure = ContractFailure::panicked("contract", Box::new(String::from("retained panic")));
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = failure.panic.lock().unwrap();
            panic!("poison panic lock");
        }));
        assert_eq!(
            failure
                .take_panic_payload()
                .unwrap()
                .downcast::<String>()
                .unwrap()
                .as_str(),
            "retained panic"
        );
        assert!(failure.take_panic_payload().is_none());
    }
}
