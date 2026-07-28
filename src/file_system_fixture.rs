// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow source-test-pair
//! Defines the provider-owned fixture required by filesystem contracts.

use qubit_fs::{
    FileSystem,
    FsPath,
};

/// Supplies an isolated filesystem and provider-specific contract paths.
///
/// Every mutating contract assertion requires a fresh fixture. Implementations
/// must retain any resource guard needed to keep the filesystem alive for the
/// duration of an assertion.
pub trait FileSystemFixture {
    /// Returns the configured filesystem under test.
    ///
    /// # Returns
    ///
    /// A filesystem that remains valid for the fixture lifetime.
    fn file_system(&self) -> &dyn FileSystem;

    /// Maps one testkit-relative path into provider path syntax.
    ///
    /// # Parameters
    ///
    /// * `relative` - Non-empty `/`-separated relative path supplied by the
    ///   testkit.
    ///
    /// # Returns
    ///
    /// The equivalent path accepted by the configured filesystem.
    fn path(&self, relative: &str) -> FsPath;

    /// Maps a testkit-relative list prefix into provider list-prefix syntax.
    ///
    /// # Parameters
    ///
    /// * `root` - Provider-local path passed as the list root.
    /// * `relative` - Non-empty `/`-separated prefix relative to `root`.
    ///
    /// # Returns
    ///
    /// The equivalent prefix accepted by the configured filesystem. Providers
    /// whose list prefix syntax matches the testkit syntax can use this
    /// default implementation.
    fn list_prefix(&self, _root: &FsPath, relative: &str) -> String {
        relative.to_owned()
    }

    /// Seeds a complete file outside the capability under test.
    ///
    /// # Parameters
    ///
    /// * `relative` - Non-empty `/`-separated testkit-relative path.
    /// * `bytes` - Complete contents to make available at that path.
    ///
    /// # Returns
    /// The seeded provider-local path when the fixture has an out-of-band setup
    /// mechanism; `None` asks the contract to use ordinary write operations.
    fn seed_file(&self, _relative: &str, _bytes: &[u8]) -> Option<FsPath> {
        None
    }

    /// Reads a complete file outside the capability under test.
    ///
    /// # Parameters
    ///
    /// * `path` - Provider-local path to observe.
    ///
    /// # Returns
    /// Complete observed bytes when the fixture has an out-of-band observation
    /// mechanism; `None` asks the contract to use ordinary read operations.
    fn read_file(&self, _path: &FsPath) -> Option<Vec<u8>> {
        None
    }

    /// Supplies one existing empty directory or prefix for representation
    /// checks.
    ///
    /// # Returns
    /// A provider-local empty directory or prefix when the fixture can prepare
    /// one; `None` means the matching representation assertion cannot run.
    fn empty_directory_path(&self) -> Option<FsPath> {
        None
    }

    /// Supplies one existing symbolic link for representation checks.
    ///
    /// # Returns
    /// A provider-local symbolic-link path when the fixture can prepare one;
    /// `None` means the matching representation assertion cannot run.
    fn symlink_path(&self) -> Option<FsPath> {
        None
    }
}
