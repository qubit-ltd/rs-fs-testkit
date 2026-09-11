// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
// Deterministic fixtures used only by executable Rustdoc examples.

#[path = "fs_rustdoc_support.rs"]
mod fs_rustdoc_support;

pub use fs_rustdoc_support::poll_support;
pub use fs_rustdoc_support::rustdoc_provider;

#[cfg(feature = "async")]
pub use fs_rustdoc_support::async_recording_spi;

use qubit_fs::FileSystem;
use qubit_fs::path::Path;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureResult;

/// Minimal synchronous fixture backed by the shared in-memory rustdoc provider.
pub struct RustdocSyncFixture {
    file_system: FileSystem,
}

impl RustdocSyncFixture {
    /// Creates an isolated fixture for Rustdoc contract-suite examples.
    pub fn new() -> Self {
        Self {
            file_system: rustdoc_provider::filesystem(),
        }
    }
}

impl FileSystemFixture for RustdocSyncFixture {
    fn file_system(&self) -> &FileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/rustdoc/{relative}"))
            .map_err(|error| FixtureError::with_source("invalid rustdoc path", error))
    }

    fn teardown(&self) -> FixtureResult<()> {
        Ok(())
    }
}

#[cfg(feature = "async")]
mod asynchronous {
    use qubit_fs::AsyncFileSystem;
    use qubit_fs::path::Path;
    use qubit_fs_testkit::AsyncFileSystemFixture;
    use qubit_fs_testkit::FixtureError;
    use qubit_fs_testkit::FixtureFuture;
    use qubit_fs_testkit::FixtureResult;

    use super::fs_rustdoc_support;
    use super::fs_rustdoc_support::async_filesystem;

    /// Minimal asynchronous fixture backed by the shared recording provider.
    pub struct RustdocAsyncFixture {
        file_system: AsyncFileSystem,
    }

    impl RustdocAsyncFixture {
        /// Creates an isolated asynchronous fixture for Rustdoc examples.
        pub fn new() -> Self {
            Self {
                file_system: async_filesystem(),
            }
        }
    }

    impl AsyncFileSystemFixture for RustdocAsyncFixture {
        fn file_system(&self) -> &AsyncFileSystem {
            &self.file_system
        }

        fn path(&self, relative: &str) -> FixtureResult<Path> {
            Path::parse(&format!("/rustdoc/{relative}"))
                .map_err(|error| FixtureError::with_source("invalid rustdoc path", error))
        }

        fn teardown(&self) -> FixtureFuture<'_, ()> {
            Box::pin(async { Ok(()) })
        }
    }
}

#[cfg(feature = "async")]
pub use asynchronous::RustdocAsyncFixture;
