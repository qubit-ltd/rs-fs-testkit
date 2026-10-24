// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Owned write requests prepared independently of the operation under test.

use qubit_fs::path::Path;
use qubit_fs::write::WriteOptions;

/// A write request; its expected behavior is determined by `WriteScenario`.
///
/// Fixtures supply representable requests, never weaker expected results.
///
/// # Examples
///
/// ```
/// use qubit_fs::path::Path;
/// use qubit_fs::write::{WriteDisposition, WriteOptions};
/// use qubit_fs_testkit::WriteFixtureCase;
///
/// let request = WriteFixtureCase::new(
///     Path::parse("/isolated/write")?,
///     b"payload".to_vec(),
///     WriteOptions::default().with_disposition(WriteDisposition::CreateNew),
/// );
/// assert_eq!(request.bytes(), b"payload");
/// # Ok::<(), qubit_fs::error::FsError>(())
/// ```
#[must_use]
#[derive(Clone, Debug)]
pub struct WriteFixtureCase {
    /// Destination path passed to the write operation.
    path: Path,
    /// Owned payload the suite will verify independently.
    bytes: Vec<u8>,
    /// Requested disposition, preconditions, and guarantees.
    options: WriteOptions,
}

impl WriteFixtureCase {
    /// Creates an owned request for the selected scenario.
    ///
    /// # Parameters
    ///
    /// * `path` - destination path owned by the request.
    /// * `bytes` - payload owned for independent verification.
    /// * `options` - requested disposition, preconditions, and guarantees.
    ///
    /// # Returns
    ///
    /// The request retaining all three supplied values.
    #[inline]
    pub fn new(path: Path, bytes: Vec<u8>, options: WriteOptions) -> Self {
        Self { path, bytes, options }
    }

    /// Returns the destination mapped into the fixture's isolated namespace.
    ///
    /// # Returns
    ///
    /// A borrow of the destination path stored in this request.
    #[must_use = "the destination path should be inspected before the request is used"]
    #[inline]
    pub const fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the payload the suite will verify independently.
    ///
    /// # Returns
    ///
    /// A borrow of the bytes stored in this request.
    #[must_use]
    #[inline]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the requested disposition, preconditions, and guarantees.
    ///
    /// # Returns
    ///
    /// A borrow of the write options stored in this request.
    #[must_use = "the write options should be inspected before the request is used"]
    #[inline]
    pub const fn options(&self) -> &WriteOptions {
        &self.options
    }
}
