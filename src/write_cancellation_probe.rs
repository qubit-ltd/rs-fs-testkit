//! Stage-aware asynchronous whole-file write cancellation probes.

use std::task::Context;
use std::task::Poll;

use crate::AsyncWriteFixtureCase;
use crate::FixtureResult;

/// Controls one provider-owned pending stage of an owning write operation.
pub trait WriteCancellationProbe: Send + Sync {
    /// Returns the isolated write request controlled by this probe.
    fn case(&self) -> &AsyncWriteFixtureCase;

    /// Polls until the configured provider stage is pending.
    fn poll_reached(&self, context: &mut Context<'_>) -> Poll<FixtureResult<()>>;

    /// Releases the provider gate without performing another filesystem call.
    fn disarm(&self) -> FixtureResult<()>;
}
