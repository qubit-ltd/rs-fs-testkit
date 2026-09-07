// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
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
pub struct ContractFailure {
    check: Option<ContractCheckId>,
    message: String,
    source: Option<Box<dyn Error + Send + Sync>>,
    panic: Mutex<Option<Box<dyn Any + Send>>>,
}

impl ContractFailure {
    /// Returns the check identity when execution could attribute the failure.
    #[must_use]
    pub const fn check(&self) -> Option<ContractCheckId> {
        self.check
    }

    /// Returns the diagnostic context without formatting its underlying cause.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Takes the original panic payload, preserving its concrete type.
    ///
    /// This transfers ownership through a borrowed run. Returns `None` when
    /// the failure was not a panic or another caller already took its payload.
    /// Taking the payload does not remove the failure from the run's history.
    pub fn take_panic_payload(&self) -> Option<Box<dyn Any + Send>> {
        self.panic.lock().unwrap_or_else(|poison| poison.into_inner()).take()
    }

    /// Attributes a preserved failure to its typed execution entry.
    pub(crate) fn at(mut self, check: ContractCheckId) -> Self {
        self.check = Some(check);
        self
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
