// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cancellation stages for asynchronous whole-file write probes.

/// A provider-owned stage at which an owning write operation may be cancelled.
///
/// # Examples
///
/// ```
/// use qubit_fs_testkit::AsyncWriteCancellationStage;
///
/// assert_eq!(AsyncWriteCancellationStage::Open, AsyncWriteCancellationStage::Open);
/// assert_ne!(AsyncWriteCancellationStage::Flush, AsyncWriteCancellationStage::Commit);
/// ```
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
