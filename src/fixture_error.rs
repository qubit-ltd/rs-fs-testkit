// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Errors raised while a fixture prepares or observes a contract resource.

use std::{
    error::Error,
    fmt::{
        Debug,
        Display,
        Formatter,
        Result as FmtResult,
    },
};

/// Failure raised by fixture setup or out-of-band observation.
#[must_use]
pub struct FixtureError {
    /// Human-readable context safe to expose through `Display` and `Debug`.
    message: String,
    /// Optional underlying failure retained for error-chain inspection.
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl FixtureError {
    /// Creates an error with a human-readable fixture failure message.
    ///
    /// # Parameters
    ///
    /// * `message` - Safe diagnostic context for the fixture failure.
    ///
    /// # Returns
    ///
    /// A fixture error without an underlying source.
    #[inline]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    /// Creates an error that preserves an underlying fixture failure as its
    /// source.
    ///
    /// # Parameters
    ///
    /// * `message` - Safe diagnostic context for the fixture failure.
    /// * `source` - Underlying error retained in the standard error chain.
    ///
    /// # Returns
    ///
    /// A fixture error wrapping the supplied source.
    #[inline]
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
    ///
    /// # Parameters
    ///
    /// * `formatter` - Destination formatter receiving the safe debug view.
    ///
    /// # Returns
    ///
    /// The formatter result.
    #[inline]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter
            .debug_struct("FixtureError")
            .field("message", &self.message)
            .finish()
    }
}

impl Display for FixtureError {
    /// Displays the fixture failure message.
    ///
    /// # Parameters
    ///
    /// * `formatter` - Destination formatter receiving the safe message.
    ///
    /// # Returns
    ///
    /// The formatter result.
    #[inline(always)]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(&self.message)
    }
}

impl Error for FixtureError {
    /// Returns the preserved fixture failure, when one was supplied.
    ///
    /// # Returns
    ///
    /// `Some(source)` when created by [`Self::with_source`], or `None` when
    /// created by [`Self::new`].
    #[inline]
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|error| error as &(dyn Error + 'static))
    }
}

/// Result returned by fixture setup and out-of-band observation hooks.
///
/// # Type Parameters
///
/// * `T` - Successful value produced by the fixture hook.
pub type FixtureResult<T> = Result<T, FixtureError>;
