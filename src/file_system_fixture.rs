// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Typed synchronous fixtures for filesystem contract suites.

use qubit_fs::{
    CopyMethod,
    FileSystem,
    Path,
};

use crate::{
    CopyFixtureCase,
    FixtureResult,
    FixtureSupport,
};

/// Supplies an isolated facade and provider-specific contract observations.
pub trait FileSystemFixture {
    /// Returns the concrete synchronous filesystem facade under test.
    fn file_system(&self) -> &FileSystem;

    /// Maps a testkit-relative name to an isolated logical path.
    fn path(&self, relative: &str) -> FixtureResult<Path>;

    /// Maps a relative list prefix for the supplied root.
    fn list_prefix(
        &self,
        root: &Path,
        relative: &str,
    ) -> FixtureResult<String> {
        let _ = root;
        Ok(relative.to_owned())
    }

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

    /// Supplies a case in which the requested native copy method must apply.
    fn copy_fast_path_case(
        &self,
        method: CopyMethod,
    ) -> FixtureResult<FixtureSupport<CopyFixtureCase>> {
        let _ = method;
        Ok(FixtureSupport::Unsupported)
    }
}
