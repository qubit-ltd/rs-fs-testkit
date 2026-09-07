// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Owned write requests prepared independently of the operation under test.

use qubit_fs::path::Path;
use qubit_fs::write::WriteOptions;

/// A write request; its expected behavior is determined by `WriteScenario`.
///
/// Fixtures supply representable requests, never weaker expected results.
#[must_use]
#[derive(Clone, Debug)]
pub struct WriteFixtureCase {
    path: Path,
    bytes: Vec<u8>,
    options: WriteOptions,
}

impl WriteFixtureCase {
    /// Creates an owned request for the selected scenario.
    pub fn new(path: Path, bytes: Vec<u8>, options: WriteOptions) -> Self {
        Self { path, bytes, options }
    }
    /// Returns the destination mapped into the fixture's isolated namespace.
    pub const fn path(&self) -> &Path {
        &self.path
    }
    /// Returns the payload the suite will verify independently.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Returns the requested disposition, preconditions, and guarantees.
    pub const fn options(&self) -> &WriteOptions {
        &self.options
    }
}
