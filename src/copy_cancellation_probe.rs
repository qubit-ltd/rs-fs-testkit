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
/// # Context
///
/// A fixture returns this probe from
/// [`crate::AsyncFileSystemFixture::prepare_copy_cancellation`] after arming
/// the requested stage. The copy execution future must be dropped before
/// calling [`disarm`](Self::disarm), so the provider stops using the gate
/// before it is released.
///
/// # Polling
///
/// [`poll_reached`](Self::poll_reached) returns `Poll::Pending` until the
/// provider reaches the requested stage. Before returning `Pending`, it must
/// arrange to wake the supplied task when the stage becomes observable.
/// `Poll::Ready(Ok(()))` confirms that the stage is gated, and
/// `Poll::Ready(Err(_))` reports that the fixture could not confirm it.
///
/// # Errors
///
/// `poll_reached` returns a fixture error when stage acknowledgement fails.
/// `disarm` returns a fixture error when the gate cannot be released. The
/// probe must release the gate even when reporting a disarm error, so fixture
/// teardown can safely continue.
///
/// # Examples
///
/// The execution future must already have been dropped before this function
/// is called.
///
/// ```
/// use qubit_fs_testkit::CopyCancellationProbe;
/// use qubit_fs_testkit::FixtureResult;
///
/// fn release_after_execution_is_dropped(probe: &dyn CopyCancellationProbe) -> FixtureResult<()> {
///     let case = probe.case();
///     assert_ne!(case.source(), case.target());
///     probe.disarm()?;
///     Ok(())
/// }
/// ```
pub trait CopyCancellationProbe: Send + Sync {
    /// Returns the isolated copy request controlled by this probe.
    fn case(&self) -> &AsyncCopyFixtureCase;

    /// Polls until the requested stage has been reached and gated.
    ///
    /// # Parameters
    ///
    /// - `context`: Task context used to register a wake-up while the stage is
    ///   still pending.
    fn poll_reached(&self, context: &mut Context<'_>) -> Poll<FixtureResult<()>>;

    /// Releases the provider-owned gate without performing I/O.
    fn disarm(&self) -> FixtureResult<()>;
}
