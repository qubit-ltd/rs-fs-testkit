// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Typed synchronous fixtures for filesystem contract suites.

use qubit_fs::{CopyMethod, FileSystem, Path, ResourceVersion};

use crate::{CopyFixtureCase, FixtureResult, FixtureSupport};

/// Supplies an isolated facade and provider-specific contract observations.
pub trait FileSystemFixture {
    /// Returns the concrete synchronous filesystem facade under test.
    ///
    /// # Returns
    ///
    /// The isolated filesystem facade owned by this fixture.
    fn file_system(&self) -> &FileSystem;

    /// Maps a testkit-relative name to an isolated logical path.
    ///
    /// # Parameters
    ///
    /// * `relative` - Suite-generated name relative to the fixture namespace.
    ///
    /// # Returns
    ///
    /// The corresponding logical path within the isolated namespace.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when the name cannot be
    /// represented by the provider's path model.
    fn path(&self, relative: &str) -> FixtureResult<Path>;

    /// Maps a relative list prefix for the supplied root.
    ///
    /// # Parameters
    ///
    /// * `root` - Logical directory passed to the list operation.
    /// * `relative` - Testkit-relative descendant selected by the contract.
    ///
    /// # Returns
    ///
    /// The provider-specific prefix expected by its list implementation.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when the prefix cannot be
    /// represented for the supplied root.
    #[inline]
    fn list_prefix(&self, root: &Path, relative: &str) -> FixtureResult<String> {
        let _ = root;
        Ok(relative.to_owned())
    }

    /// Seeds a complete file outside the operation currently under test.
    ///
    /// # Parameters
    ///
    /// * `relative` - Testkit-relative path for the seeded file.
    /// * `bytes` - Exact content to publish.
    ///
    /// # Returns
    ///
    /// `Supported(path)` when seeding succeeds, or `Unsupported` when the
    /// fixture cannot prepare files out of band.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when provider-specific
    /// setup fails.
    #[inline]
    fn seed_file(&self, relative: &str, bytes: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        let _ = (relative, bytes);
        Ok(FixtureSupport::Unsupported)
    }

    /// Reads a complete file outside the operation currently under test.
    ///
    /// # Parameters
    ///
    /// * `path` - Logical path to observe.
    ///
    /// # Returns
    ///
    /// `Supported(bytes)` with the complete content, or `Unsupported` when the
    /// fixture cannot observe files out of band.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when provider-specific
    /// observation fails.
    #[inline]
    fn read_file(&self, path: &Path) -> FixtureResult<FixtureSupport<Vec<u8>>> {
        let _ = path;
        Ok(FixtureSupport::Unsupported)
    }

    /// Observes the current provider version outside the operation under test.
    #[inline]
    fn resource_version(&self, path: &Path) -> FixtureResult<FixtureSupport<ResourceVersion>> {
        let _ = path;
        Ok(FixtureSupport::Unsupported)
    }

    /// Seeds an empty directory or prefix outside the operation under test.
    #[inline]
    fn seed_empty_directory(&self, relative: &str) -> FixtureResult<FixtureSupport<Path>> {
        let _ = relative;
        Ok(FixtureSupport::Unsupported)
    }

    /// Seeds a symbolic link outside the operation under test.
    #[inline]
    fn seed_symlink(&self, relative: &str) -> FixtureResult<FixtureSupport<Path>> {
        let _ = relative;
        Ok(FixtureSupport::Unsupported)
    }

    /// Supplies a case in which the requested native copy method must apply.
    ///
    /// # Parameters
    ///
    /// * `method` - Native copy method the prepared request must exercise.
    ///
    /// # Returns
    ///
    /// `Supported(case)` when the fixture can prepare an applicable request,
    /// or `Unsupported` when no such provider-specific case is available.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when provider-specific
    /// case preparation fails.
    #[inline]
    fn copy_fast_path_case(
        &self,
        method: CopyMethod,
    ) -> FixtureResult<FixtureSupport<CopyFixtureCase>> {
        let _ = method;
        Ok(FixtureSupport::Unsupported)
    }
}
