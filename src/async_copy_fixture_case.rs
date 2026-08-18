// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Provider-prepared asynchronous copy fixture cases.

use qubit_fs::copy::CopyOptions;
use qubit_fs::path::Path;

/// Provider-prepared asynchronous copy request used for cancellation probing.
#[must_use]
#[derive(Clone, Debug)]
pub struct AsyncCopyFixtureCase {
    /// Source path passed to the copy operation.
    source: Path,
    /// Destination path passed to the copy operation.
    target: Path,
    /// Options governing the probed copy operation.
    options: CopyOptions,
}

impl AsyncCopyFixtureCase {
    /// Creates a cancellation probe request from owned copy arguments.
    ///
    /// # Parameters
    ///
    /// * `source` - Source path passed to the copy operation.
    /// * `target` - Destination path passed to the copy operation.
    /// * `options` - Options governing the copy operation.
    ///
    /// # Returns
    ///
    /// A prepared request containing the supplied arguments.
    #[inline]
    pub fn new(source: Path, target: Path, options: CopyOptions) -> Self {
        Self {
            source,
            target,
            options,
        }
    }

    /// Returns the source path without transferring ownership.
    ///
    /// # Returns
    ///
    /// The prepared source path.
    #[inline(always)]
    #[must_use]
    pub const fn source(&self) -> &Path {
        &self.source
    }

    /// Returns the target path without transferring ownership.
    ///
    /// # Returns
    ///
    /// The prepared destination path.
    #[inline(always)]
    #[must_use]
    pub const fn target(&self) -> &Path {
        &self.target
    }

    /// Returns the copy options without transferring ownership.
    ///
    /// # Returns
    ///
    /// The prepared copy options.
    #[inline(always)]
    #[must_use]
    pub const fn options(&self) -> &CopyOptions {
        &self.options
    }

    /// Decomposes the prepared request into owned parts.
    ///
    /// # Returns
    ///
    /// The source path, destination path, and copy options in that order.
    #[inline]
    #[must_use]
    pub fn into_parts(self) -> (Path, Path, CopyOptions) {
        (self.source, self.target, self.options)
    }
}
