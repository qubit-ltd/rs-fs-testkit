// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Provider-prepared asynchronous whole-file write requests.

use qubit_fs::path::Path;
use qubit_fs::write::WriteOptions;

/// Owned request arguments for an asynchronous write cancellation probe.
///
/// Retain these arguments until the borrowed execution future is dropped.
/// `into_parts` transfers ownership to the driver; the probe can keep an
/// independent clone for target observation.
///
/// ```
/// use qubit_fs::path::Path;
/// use qubit_fs::write::{WriteDisposition, WriteOptions};
/// use qubit_fs_testkit::AsyncWriteFixtureCase;
///
/// let request = AsyncWriteFixtureCase::new(
///     Path::parse("/isolated/write-cancel")?,
///     b"payload".to_vec(),
///     WriteOptions::default().with_disposition(WriteDisposition::CreateNew),
/// );
/// assert_eq!(request.path().as_str(), "/isolated/write-cancel");
/// assert_eq!(request.bytes(), b"payload");
/// assert_eq!(request.options().disposition(), WriteDisposition::CreateNew);
/// let (path, bytes, options) = request.into_parts();
/// assert_eq!(path.as_str(), "/isolated/write-cancel");
/// assert_eq!(bytes, b"payload");
/// assert_eq!(options.disposition(), WriteDisposition::CreateNew);
/// # Ok::<(), qubit_fs::error::FsError>(())
/// ```
#[must_use]
#[derive(Clone, Debug)]
pub struct AsyncWriteFixtureCase {
    /// Destination path passed to the operation.
    path: Path,
    /// Owned bytes transferred into the write operation.
    bytes: Vec<u8>,
    /// Publication and durability options.
    options: WriteOptions,
}

impl AsyncWriteFixtureCase {
    /// Creates a prepared write request.
    pub fn new(path: Path, bytes: Vec<u8>, options: WriteOptions) -> Self {
        Self { path, bytes, options }
    }

    /// Returns the destination path.
    #[must_use]
    pub const fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the payload.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the write options.
    #[must_use]
    pub const fn options(&self) -> &WriteOptions {
        &self.options
    }

    /// Decomposes the request into owned parts.
    #[must_use]
    pub fn into_parts(self) -> (Path, Vec<u8>, WriteOptions) {
        (self.path, self.bytes, self.options)
    }
}
