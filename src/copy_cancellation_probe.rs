// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Stage-aware asynchronous copy cancellation probes.

use std::task::Context;
use std::task::Poll;

use crate::AsyncCopyFixtureCase;
use crate::FixtureResult;

/// Observes and controls one provider-owned pending copy stage.
pub trait CopyCancellationProbe: Send + Sync {
    /// Returns the isolated copy request controlled by this probe.
    fn case(&self) -> &AsyncCopyFixtureCase;

    /// Polls until the requested stage has been reached and gated.
    fn poll_reached(&self, context: &mut Context<'_>) -> Poll<FixtureResult<()>>;

    /// Releases the provider-owned gate without performing I/O.
    fn disarm(&self) -> FixtureResult<()>;
}
