// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Typed asynchronous fixtures for filesystem contract suites.

use std::{future::Future, pin::Pin};

use qubit_fs::{AsyncFileSystem, Path};

use crate::{AsyncCopyCancellationStage, AsyncCopyFixtureCase, FixtureResult, FixtureSupport};

/// Runtime-neutral future returned by asynchronous fixture observations.
pub type FixtureFuture<'a, T> = Pin<Box<dyn Future<Output = FixtureResult<T>> + Send + 'a>>;

/// Supplies an isolated asynchronous facade and optional provider observations.
pub trait AsyncFileSystemFixture: Sync {
    /// Returns the concrete asynchronous filesystem facade under test.
    fn file_system(&self) -> &AsyncFileSystem;

    /// Maps a testkit-relative name to an isolated logical path.
    fn path(&self, relative: &str) -> FixtureResult<Path>;

    /// Maps a relative list prefix for the supplied root.
    fn list_prefix(&self, root: &Path, relative: &str) -> FixtureResult<String> {
        let _ = root;
        Ok(relative.to_owned())
    }

    /// Asynchronously seeds a complete file outside the operation under test.
    fn seed_file<'a>(
        &'a self,
        relative: &'a str,
        bytes: &'a [u8],
    ) -> FixtureFuture<'a, FixtureSupport<Path>> {
        let _ = (relative, bytes);
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Asynchronously observes a complete file outside the operation under
    /// test.
    fn read_file<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<Vec<u8>>> {
        let _ = path;
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Supplies a cancellation probe with provider-owned pending-stage
    /// controls.
    fn copy_cancellation_case(
        &self,
        stage: AsyncCopyCancellationStage,
    ) -> FixtureResult<FixtureSupport<AsyncCopyFixtureCase>> {
        let _ = stage;
        Ok(FixtureSupport::Unsupported)
    }
}
