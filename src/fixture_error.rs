// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow all -- contract behavior is covered by the conforming and
// fault matrices.
//! Errors raised while a fixture prepares or observes a contract resource.

use std::{
    error::Error,
    fmt::{Debug, Display, Formatter, Result as FmtResult},
};

/// Failure raised by fixture setup or out-of-band observation.
pub struct FixtureError {
    message: String,
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl FixtureError {
    /// Creates an error with a human-readable fixture failure message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    /// Creates an error that preserves an underlying fixture failure as its
    /// source.
    #[must_use]
    pub fn with_source(
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

impl Debug for FixtureError {
    /// Formats the error without requiring its source to implement `Debug`.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter
            .debug_struct("FixtureError")
            .field("message", &self.message)
            .finish()
    }
}

impl Display for FixtureError {
    /// Displays the fixture failure message.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(&self.message)
    }
}

impl Error for FixtureError {
    /// Returns the preserved fixture failure, when one was supplied.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|error| error as &(dyn Error + 'static))
    }
}

/// Result returned by fixture setup and out-of-band observation hooks.
pub type FixtureResult<T> = Result<T, FixtureError>;
