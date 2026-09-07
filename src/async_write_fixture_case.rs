//! Provider-prepared asynchronous whole-file write requests.

use qubit_fs::path::Path;
use qubit_fs::write::WriteOptions;

/// Owned request arguments for an asynchronous write cancellation probe.
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
