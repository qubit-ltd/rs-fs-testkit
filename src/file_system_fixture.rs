// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow source-test-pair
//! Defines the provider-owned fixture required by filesystem contracts.

use qubit_fs::{FileSystem, FsPath};

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
}
