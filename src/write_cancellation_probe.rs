// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stage-aware asynchronous whole-file write cancellation probes.

use std::task::Context;
use std::task::Poll;

use crate::AsyncWriteFixtureCase;
use crate::FixtureFuture;
use crate::FixtureResult;

/// Controls one provider-owned pending stage of an owning write operation.
///
/// # Examples
///
/// ```
/// use qubit_fs::path::Path;
/// use qubit_fs::write::{WriteDisposition, WriteOptions};
/// use qubit_fs_testkit::AsyncWriteFixtureCase;
///
/// let case = AsyncWriteFixtureCase::new(
///     Path::parse("/target")?,
///     b"payload".to_vec(),
///     WriteOptions::default().with_disposition(WriteDisposition::CreateNew),
/// );
/// assert_eq!(case.bytes(), b"payload");
/// # Ok::<(), qubit_fs::error::FsError>(())
/// ```
pub trait WriteCancellationProbe: Send + Sync {
    /// Returns the isolated write request controlled by this probe.
    fn case(&self) -> &AsyncWriteFixtureCase;

    /// Polls until the configured provider stage is pending.
    fn poll_reached(&self, context: &mut Context<'_>) -> Poll<FixtureResult<()>>;

    /// Returns the independently observed bytes accepted before cancellation.
    ///
    /// Count bytes accepted by the provider writer, including unpublished
    /// staging bytes. Opening a writer has accepted zero payload bytes.
    fn accepted_bytes(&self) -> FixtureResult<u64>;

    /// Observes the target independently of the filesystem under test.
    ///
    /// Return `None` only when absence is confirmed; otherwise return the
    /// complete published bytes. Errors must remain errors. The suite compares
    /// observations before execution and after a `NotPublished` recovery, so
    /// neither a stale cache nor an assumed empty target is sufficient.
    fn observe_target(&self) -> FixtureFuture<'_, Option<Vec<u8>>>;

    /// Releases the provider gate without performing another filesystem call.
    fn disarm(&self) -> FixtureResult<()>;
}
