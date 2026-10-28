// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Original writer errors paired with their still-owned recovery session.

use std::error::Error;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;

/// A failed streaming operation whose writer remains available for recovery.
///
/// `W` is the synchronous or asynchronous writer, or an owning asynchronous
/// write/copy operation retained after a cancellation diagnostic failure.
/// Obtain this value through [`crate::ContractSource::take`] and downcast to
/// the concrete type. For `ContractWriterFailure<AsyncWriteAllOperation>` or
/// `ContractWriterFailure<AsyncCopyOperation>`, `error()` contains the original
/// diagnostic [`crate::ContractFailure`], and `writer_mut()` exposes the
/// operation. Call its `take_recovery()` and explicitly abort the returned
/// writer when recovery is required. Taking the source transfers recovery
/// responsibility to the caller. Formatting does not inspect the retained
/// value or invoke a provider error formatter.
///
/// # Examples
///
/// Downcast the error returned from [`crate::ContractSource::take`] to the
/// concrete writer failure, then inspect the retained writer:
///
/// ```
/// use qubit_fs_testkit::ContractWriterFailure;
///
/// fn inspect_writer<W>(failure: &ContractWriterFailure<W>) -> &W {
///     failure.writer()
/// }
/// ```
#[must_use]
pub struct ContractWriterFailure<W> {
    error: Box<dyn Error + Send>,
    writer: W,
}

impl<W> ContractWriterFailure<W> {
    /// Retains a streaming failure before its writer can leave scope.
    pub(crate) fn new(error: impl Error + Send + 'static, writer: W) -> Self {
        Self {
            error: Box::new(error),
            writer,
        }
    }

    /// Returns the original error for typed inspection.
    #[must_use = "inspect the original error or intentionally ignore it"]
    #[inline]
    pub fn error(&self) -> &(dyn Error + Send + 'static) {
        self.error.as_ref()
    }

    /// Borrows the retained writer for lifecycle inspection.
    #[must_use = "inspect the retained recovery writer or intentionally ignore it"]
    #[inline]
    pub const fn writer(&self) -> &W {
        &self.writer
    }

    /// Borrows the retained writer for explicit recovery or abort.
    #[must_use = "use the mutable writer reference to recover or abort the writer"]
    #[inline]
    pub fn writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Transfers both the original error and its writer to the caller.
    #[must_use = "recover or intentionally discard the original error and writer"]
    pub fn into_parts(self) -> (Box<dyn Error + Send>, W) {
        (self.error, self.writer)
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
