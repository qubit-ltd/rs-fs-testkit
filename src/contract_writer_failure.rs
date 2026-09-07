// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Original writer errors paired with their still-owned recovery session.

use std::error::Error;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;

/// A failed streaming operation whose writer remains available for recovery.
///
/// `W` is the synchronous or asynchronous writer. Obtain this value through
/// [`crate::ContractSource::take`] and downcast to the concrete writer type.
/// Taking it transfers recovery responsibility to the caller. Formatting does
/// not inspect the writer or invoke the provider's error formatter.
#[must_use]
pub struct ContractWriterFailure<W> {
    error: Box<dyn Error + Send>,
    writer: W,
}

impl<W> ContractWriterFailure<W> {
    /// Returns the original error for typed inspection.
    pub fn error(&self) -> &(dyn Error + Send + 'static) {
        self.error.as_ref()
    }

    /// Borrows the retained writer for lifecycle inspection.
    pub const fn writer(&self) -> &W {
        &self.writer
    }

    /// Borrows the retained writer for explicit recovery or abort.
    pub fn writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Transfers both the original error and its writer to the caller.
    pub fn into_parts(self) -> (Box<dyn Error + Send>, W) {
        (self.error, self.writer)
    }

    /// Retains a streaming failure before its writer can leave scope.
    pub(crate) fn new(error: impl Error + Send + 'static, writer: W) -> Self {
        Self {
            error: Box::new(error),
            writer,
        }
    }
}

impl<W> Debug for ContractWriterFailure<W> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("ContractWriterFailure { writer: retained, error: retained }")
    }
}

impl<W> Display for ContractWriterFailure<W> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("streaming writer failed; original error and recovery writer retained")
    }
}

impl<W> Error for ContractWriterFailure<W> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.error.as_ref())
    }
}
