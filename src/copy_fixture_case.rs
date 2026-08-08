// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Provider-prepared synchronous copy fixture cases.

use qubit_fs::CopyOptions;
use qubit_fs::Path;

/// Provider-prepared case that makes one native copy method applicable.
#[must_use]
#[derive(Clone, Debug)]
pub struct CopyFixtureCase {
    /// Source path passed to the copy operation.
    source: Path,
    /// Destination path passed to the copy operation.
    target: Path,
    /// Options that make the native copy method applicable.
    options: CopyOptions,
}

impl CopyFixtureCase {
    /// Creates a native-copy case from its source, target, and requested
    /// options.
    ///
    /// # Parameters
    ///
    /// * `source` - Source path passed to the copy operation.
    /// * `target` - Destination path passed to the copy operation.
    /// * `options` - Options that make the native method applicable.
    ///
    /// # Returns
    ///
    /// A prepared case containing the supplied arguments.
    #[inline]
    pub fn new(source: Path, target: Path, options: CopyOptions) -> Self {
        Self {
            source,
            target,
            options,
        }
    }

    /// Returns the prepared source path without transferring ownership.
    ///
    /// # Returns
    ///
    /// The prepared source path.
    #[inline(always)]
    #[must_use]
    pub const fn source(&self) -> &Path {
        &self.source
    }

    /// Returns the prepared target path without transferring ownership.
    ///
    /// # Returns
    ///
    /// The prepared destination path.
    #[inline(always)]
    #[must_use]
    pub const fn target(&self) -> &Path {
        &self.target
    }

    /// Returns the options that make the case applicable.
    ///
    /// # Returns
    ///
    /// The prepared copy options.
    #[inline(always)]
    #[must_use]
    pub const fn options(&self) -> &CopyOptions {
        &self.options
    }

    /// Decomposes the case into its owned request parts.
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
