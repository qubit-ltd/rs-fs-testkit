// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Typed asynchronous fixtures for filesystem contract suites.

use std::{future::Future, pin::Pin};

use qubit_fs::{AsyncFileSystem, CopyMethod, Path, ResourceVersion};

use crate::{
    AsyncCopyCancellationStage, AsyncCopyFixtureCase, CopyFixtureCase, FixtureResult,
    FixtureSupport,
};

/// Runtime-neutral future returned by asynchronous fixture observations.
///
/// # Type Parameters
///
/// * `'a` - Lifetime shared by the fixture and borrowed request data.
/// * `T` - Successful value produced by the asynchronous hook.
pub type FixtureFuture<'a, T> = Pin<Box<dyn Future<Output = FixtureResult<T>> + Send + 'a>>;

/// Supplies an isolated asynchronous facade and optional provider observations.
pub trait AsyncFileSystemFixture: Sync {
    /// Returns the concrete asynchronous filesystem facade under test.
    ///
    /// # Returns
    ///
    /// The isolated asynchronous facade owned by this fixture.
    fn file_system(&self) -> &AsyncFileSystem;

    /// Maps a testkit-relative name to an isolated logical path.
    ///
    /// # Parameters
    ///
    /// * `relative` - Suite-generated name relative to the fixture namespace.
    ///
    /// # Returns
    ///
    /// The corresponding logical path within the isolated namespace.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when the name cannot be
    /// represented by the provider's path model.
    fn path(&self, relative: &str) -> FixtureResult<Path>;

    /// Maps a relative list prefix for the supplied root.
    ///
    /// # Parameters
    ///
    /// * `root` - Logical directory passed to the list operation.
    /// * `relative` - Testkit-relative descendant selected by the contract.
    ///
    /// # Returns
    ///
    /// The provider-specific prefix expected by its list implementation.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when the prefix cannot be
    /// represented for the supplied root.
    #[inline]
    fn list_prefix(&self, root: &Path, relative: &str) -> FixtureResult<String> {
        let _ = root;
        Ok(relative.to_owned())
    }

    /// Asynchronously seeds a complete file outside the operation under test.
    ///
    /// # Parameters
    ///
    /// * `relative` - Testkit-relative path for the seeded file.
    /// * `bytes` - Exact content to publish.
    ///
    /// # Returns
    ///
    /// A future resolving to `Supported(path)` when seeding succeeds, or
    /// `Unsupported` when the fixture cannot prepare files out of band.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when
    /// provider-specific setup fails.
    #[inline]
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
    ///
    /// # Parameters
    ///
    /// * `path` - Logical path to observe.
    ///
    /// # Returns
    ///
    /// A future resolving to `Supported(bytes)` with complete content, or
    /// `Unsupported` when the fixture cannot observe files out of band.
    ///
    /// # Errors
    ///
    /// The future returns [`FixtureError`](crate::FixtureError) when
    /// provider-specific observation fails.
    #[inline]
    fn read_file<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<Vec<u8>>> {
        let _ = path;
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Asynchronously observes the current provider resource version.
    #[inline]
    fn resource_version<'a>(
        &'a self,
        path: &'a Path,
    ) -> FixtureFuture<'a, FixtureSupport<ResourceVersion>> {
        let _ = path;
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Asynchronously seeds an empty directory or prefix.
    #[inline]
    fn seed_empty_directory<'a>(
        &'a self,
        relative: &'a str,
    ) -> FixtureFuture<'a, FixtureSupport<Path>> {
        let _ = relative;
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Asynchronously seeds a symbolic link.
    #[inline]
    fn seed_symlink<'a>(&'a self, relative: &'a str) -> FixtureFuture<'a, FixtureSupport<Path>> {
        let _ = relative;
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Supplies an asynchronously prepared native copy fast-path case.
    #[inline]
    fn copy_fast_path_case<'a>(
        &'a self,
        method: CopyMethod,
    ) -> FixtureFuture<'a, FixtureSupport<CopyFixtureCase>> {
        let _ = method;
        Box::pin(async { Ok(FixtureSupport::Unsupported) })
    }

    /// Supplies a cancellation probe with provider-owned pending-stage
    /// controls.
    ///
    /// # Parameters
    ///
    /// * `stage` - Copy stage that must remain pending until cancellation.
    ///
    /// # Returns
    ///
    /// `Supported(case)` when the fixture can prepare the requested probe, or
    /// `Unsupported` when cancellation control is unavailable.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`](crate::FixtureError) when provider-specific
    /// probe preparation fails.
    #[inline]
    fn copy_cancellation_case(
        &self,
        stage: AsyncCopyCancellationStage,
    ) -> FixtureResult<FixtureSupport<AsyncCopyFixtureCase>> {
        let _ = stage;
        Ok(FixtureSupport::Unsupported)
    }
}
