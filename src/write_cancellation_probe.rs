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
/// use qubit_fs_testkit::FixtureResult;
/// use qubit_fs_testkit::WriteCancellationProbe;
///
/// fn release_probe(probe: &dyn WriteCancellationProbe) -> FixtureResult<()> {
///     // The fixture's `prepare_write_cancellation` method provides this probe
///     // when it returns `FixtureSupport::Supported`.
///     probe.disarm()?;
///     Ok(())
/// }
/// ```
pub trait WriteCancellationProbe: Send + Sync {
    /// Returns the isolated write request controlled by this probe.
    ///
    /// # Returns
    ///
    /// The owned request whose path, bytes, and options match the operation
    /// controlled by this probe.
    #[must_use = "the isolated write request must be inspected or retained"]
    fn case(&self) -> &AsyncWriteFixtureCase;

    /// Polls until the configured provider stage is pending.
    ///
    /// # Parameters
    ///
    /// * `context` - Task context used to arrange another poll when progress
    ///   depends on asynchronous provider work.
    ///
    /// # Returns
    ///
    /// `Poll::Pending` while the operation has not reached the stage, or
    /// `Poll::Ready(Ok(()))` after the provider has gated it at that stage.
    /// A ready error reports that stage observation failed.
    fn poll_reached(&self, context: &mut Context<'_>) -> Poll<FixtureResult<()>>;

    /// Returns the independently observed bytes accepted before cancellation.
    ///
    /// Count bytes accepted by the provider writer, including unpublished
    /// staging bytes. Opening a writer has accepted zero payload bytes.
    ///
    /// # Returns
    ///
    /// The accepted payload byte count.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot determine the accepted count.
    fn accepted_bytes(&self) -> FixtureResult<u64>;

    /// Observes the target independently of the filesystem under test.
    ///
    /// Return `None` only when absence is confirmed; otherwise return the
    /// complete published bytes. Errors must remain errors. The suite compares
    /// observations before execution and after a `NotPublished` recovery, so
    /// neither a stale cache nor an assumed empty target is sufficient.
    ///
    /// # Returns
    ///
    /// A future that resolves to `None` for confirmed absence or `Some(bytes)`
    /// for the complete published target contents.
    ///
    /// # Errors
    ///
    /// Returns an error when the target cannot be observed independently.
    fn observe_target(&self) -> FixtureFuture<'_, Option<Vec<u8>>>;

    /// Releases the provider gate without performing another filesystem call.
    ///
    /// # Returns
    ///
    /// `Ok(())` when the gate is released.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot release the gate.
    fn disarm(&self) -> FixtureResult<()>;
}
