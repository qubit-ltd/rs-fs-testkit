// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Provider-prepared synchronous copy fixture cases.

use qubit_fs::{CopyOptions, Path};

/// Provider-prepared case that makes one native copy method applicable.
#[derive(Clone, Debug)]
pub struct CopyFixtureCase {
    source: Path,
    target: Path,
    options: CopyOptions,
}

impl CopyFixtureCase {
    /// Creates a native-copy case from its source, target, and requested options.
    #[must_use]
    pub fn new(source: Path, target: Path, options: CopyOptions) -> Self {
        Self {
            source,
            target,
            options,
        }
    }

    /// Returns the prepared source path.
    #[must_use]
    pub const fn source(&self) -> &Path {
        &self.source
    }

    /// Returns the prepared target path.
    #[must_use]
    pub const fn target(&self) -> &Path {
        &self.target
    }

    /// Returns the copy options that make the case applicable.
    #[must_use]
    pub const fn options(&self) -> &CopyOptions {
        &self.options
    }

    /// Decomposes the case into its owned request parts.
    #[must_use]
    pub fn into_parts(self) -> (Path, Path, CopyOptions) {
        (self.source, self.target, self.options)
    }
}
