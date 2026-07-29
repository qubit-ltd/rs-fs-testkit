// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow all -- contract behavior is covered by the conforming and
// fault matrices.
//! Typed asynchronous fixtures for filesystem contract suites.

use std::{future::Future, pin::Pin};

use qubit_fs::{AsyncFileSystem, CopyMethod, CopyOptions, Path};

use crate::{CopyFixtureCase, FixtureEntryIdentity, FixtureResult, FixtureSupport};

/// Runtime-neutral future returned by asynchronous fixture observations.
pub type FixtureFuture<'a, T> = Pin<Box<dyn Future<Output = FixtureResult<T>> + Send + 'a>>;

/// Cancellation point exercised by an asynchronous copy contract probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AsyncCopyCancellationStage {
    /// The provider-native copy attempt is pending.
    NativeAttempt,
    /// The fallback source reader is pending.
    Reader,
    /// The fallback target writer is pending.
    Writer,
    /// The fallback commit is pending.
    Commit,
}

/// Provider-prepared asynchronous copy request used for cancellation probing.
#[derive(Clone, Debug)]
pub struct AsyncCopyFixtureCase {
    source: Path,
    target: Path,
    options: CopyOptions,
}

impl AsyncCopyFixtureCase {
    /// Creates a cancellation probe request from owned copy arguments.
    #[must_use]
    pub fn new(source: Path, target: Path, options: CopyOptions) -> Self {
        Self {
            source,
            target,
            options,
        }
    }

    /// Returns the source path.
    #[must_use]
    pub const fn source(&self) -> &Path {
        &self.source
    }

    /// Returns the target path.
    #[must_use]
    pub const fn target(&self) -> &Path {
        &self.target
    }

    /// Returns the copy options.
    #[must_use]
    pub const fn options(&self) -> &CopyOptions {
        &self.options
    }

    /// Decomposes the prepared request into owned parts.
    #[must_use]
    pub fn into_parts(self) -> (Path, Path, CopyOptions) {
        (self.source, self.target, self.options)
    }
}

/// Supplies an isolated asynchronous facade and optional provider observations.
pub trait AsyncFileSystemFixture: Sync {
    /// Returns the concrete asynchronous filesystem facade under test.
    fn file_system(&self) -> &AsyncFileSystem;

    /// Maps a testkit-relative name to an isolated logical path.
    fn path(&self, relative: &str) -> FixtureResult<Path>;

    /// Maps a relative list prefix for the supplied root.
    fn list_prefix(&self, root: &Path, relative: &str) -> FixtureResult<String>;

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

    /// Supplies a known empty directory for representation assertions.
    fn empty_directory_path(&self) -> FixtureResult<FixtureSupport<Path>> {
        Ok(FixtureSupport::Unsupported)
    }

    /// Supplies a known symbolic-link path for representation assertions.
    fn symlink_path(&self) -> FixtureResult<FixtureSupport<Path>> {
        Ok(FixtureSupport::Unsupported)
    }

    /// Supplies a case in which the requested native copy method must apply.
    fn copy_fast_path_case(
        &self,
        method: CopyMethod,
    ) -> FixtureResult<FixtureSupport<CopyFixtureCase>> {
        let _ = method;
        Ok(FixtureSupport::Unsupported)
    }

    /// Asynchronously supplies an opaque stable identity for a provider entry.
    fn entry_identity<'a>(
        &'a self,
        path: &'a Path,
    ) -> FixtureFuture<'a, FixtureSupport<FixtureEntryIdentity>> {
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
