// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Fixtures for asynchronous filesystem provider contracts.

use qubit_fs::{AsyncFileSystem, FsPath};

/// Supplies an asynchronous filesystem and isolated provider-local paths.
pub trait AsyncFileSystemFixture: Sync {
    /// Returns the filesystem under test.
    ///
    /// # Returns
    /// Borrowed asynchronous filesystem implementation.
    fn file_system(&self) -> &dyn AsyncFileSystem;

    /// Builds an isolated provider-local path for one contract resource.
    ///
    /// # Parameters
    /// - `relative`: Contract-specific relative name.
    ///
    /// # Returns
    /// A provider-local path owned by this fixture.
    fn path(&self, relative: &str) -> FsPath;
}
