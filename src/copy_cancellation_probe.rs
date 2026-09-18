// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stage-aware asynchronous copy cancellation probes.

use std::task::Context;
use std::task::Poll;

use crate::AsyncCopyFixtureCase;
use crate::FixtureResult;

/// Observes and controls one provider-owned pending copy stage.
///
/// # Examples
///
/// ```
/// use qubit_fs::copy::CopyOptions;
/// use qubit_fs::path::Path;
/// use qubit_fs_testkit::AsyncCopyFixtureCase;
///
/// let case = AsyncCopyFixtureCase::new(
///     Path::parse("/source")?,
///     Path::parse("/target")?,
///     CopyOptions::file(),
/// );
/// assert_eq!(case.target().as_str(), "/target");
/// # Ok::<(), qubit_fs::error::FsError>(())
/// ```
pub trait CopyCancellationProbe: Send + Sync {
    /// Returns the isolated copy request controlled by this probe.
    fn case(&self) -> &AsyncCopyFixtureCase;

    /// Polls until the requested stage has been reached and gated.
    fn poll_reached(&self, context: &mut Context<'_>) -> Poll<FixtureResult<()>>;

    /// Releases the provider-owned gate without performing I/O.
    fn disarm(&self) -> FixtureResult<()>;
}
