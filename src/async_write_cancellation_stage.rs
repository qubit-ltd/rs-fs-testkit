//! Cancellation stages for asynchronous whole-file write probes.

/// A provider-owned stage at which an owning write operation may be cancelled.
#[must_use]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AsyncWriteCancellationStage {
    /// Opening the provider writer is pending.
    Open,
    /// Writing the payload is pending.
    Write,
    /// Flushing buffered bytes is pending.
    Flush,
    /// Publishing the completed writer is pending.
    Commit,
}
