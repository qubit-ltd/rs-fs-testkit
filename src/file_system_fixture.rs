// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow all -- contract behavior is covered by the conforming and
// fault matrices.
//! Typed synchronous fixtures for filesystem contract suites.

use qubit_fs::{
    CopyMethod,
    CopyOptions,
    FileSystem,
    Path,
};

use crate::{
    FixtureResult,
    FixtureSupport,
};

/// Opaque provider identity used to compare entries across namespace changes.
#[derive(Debug, Eq, PartialEq)]
pub struct FixtureEntryIdentity(Vec<u8>);

impl FixtureEntryIdentity {
    /// Creates an opaque identity from provider-owned stable bytes.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

/// Provider-prepared case that makes one native copy method applicable.
#[derive(Clone, Debug)]
pub struct CopyFixtureCase {
    source: Path,
    target: Path,
    options: CopyOptions,
}

impl CopyFixtureCase {
    /// Creates a native-copy case from its source, target, and requested
    /// options.
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

    /// Decomposes this case into its owned request parts.
    #[must_use]
    pub fn into_parts(self) -> (Path, Path, CopyOptions) {
        (self.source, self.target, self.options)
    }
}

/// Supplies an isolated facade and provider-specific contract observations.
pub trait FileSystemFixture {
    /// Returns the concrete synchronous filesystem facade under test.
    fn file_system(&self) -> &FileSystem;

    /// Maps a testkit-relative name to an isolated logical path.
    fn path(&self, relative: &str) -> FixtureResult<Path>;

    /// Maps a relative list prefix for the supplied root.
    fn list_prefix(&self, root: &Path, relative: &str)
    -> FixtureResult<String>;

    /// Seeds a complete file outside the operation currently under test.
    fn seed_file(
        &self,
        relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<FixtureSupport<Path>> {
        let _ = (relative, bytes);
        Ok(FixtureSupport::Unsupported)
    }

    /// Reads a complete file outside the operation currently under test.
    fn read_file(&self, path: &Path) -> FixtureResult<FixtureSupport<Vec<u8>>> {
        let _ = path;
        Ok(FixtureSupport::Unsupported)
    }

    /// Supplies a known empty directory for representation assertions.
    fn empty_directory_path(&self) -> FixtureResult<FixtureSupport<Path>> {
        Ok(FixtureSupport::Unsupported)
    }

    /// Supplies a known symbolic-link path for representation assertions.
    fn symlink_path(&self) -> FixtureResult<FixtureSupport<Path>> {
        Ok(FixtureSupport::Unsupported)
    }

    /// Supplies a case in which the requested native copy method must apply.
    fn copy_fast_path_case(
        &self,
        method: CopyMethod,
    ) -> FixtureResult<FixtureSupport<CopyFixtureCase>> {
        let _ = method;
        Ok(FixtureSupport::Unsupported)
    }

    /// Supplies an opaque stable identity for a provider entry.
    fn entry_identity(
        &self,
        path: &Path,
    ) -> FixtureResult<FixtureSupport<FixtureEntryIdentity>> {
        let _ = path;
        Ok(FixtureSupport::Unsupported)
    }
}
