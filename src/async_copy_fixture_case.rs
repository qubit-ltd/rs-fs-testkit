// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Provider-prepared asynchronous copy cancellation cases.

use qubit_fs::{CopyOptions, Path};

/// Cancellation point exercised by an asynchronous copy contract probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AsyncCopyCancellationStage {
    /// The provider-native copy attempt is pending.
    NativeAttempt,
    /// The fallback source reader is pending.
    Reader,
    /// The fallback target writer is pending.
    Writer,
    /// The fallback commit is pending.
    Commit,
}

/// Provider-prepared asynchronous copy request used for cancellation probing.
#[derive(Clone, Debug)]
pub struct AsyncCopyFixtureCase {
    source: Path,
    target: Path,
    options: CopyOptions,
}

impl AsyncCopyFixtureCase {
    /// Creates a cancellation probe request from owned copy arguments.
    #[must_use]
    pub fn new(source: Path, target: Path, options: CopyOptions) -> Self {
        Self {
            source,
            target,
            options,
        }
    }

    /// Returns the source path.
    #[must_use]
    pub const fn source(&self) -> &Path {
        &self.source
    }

    /// Returns the target path.
    #[must_use]
    pub const fn target(&self) -> &Path {
        &self.target
    }

    /// Returns the copy options.
    #[must_use]
    pub const fn options(&self) -> &CopyOptions {
        &self.options
    }

    /// Decomposes the prepared request into owned parts.
    #[must_use]
    pub fn into_parts(self) -> (Path, Path, CopyOptions) {
        (self.source, self.target, self.options)
    }
}
