// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Independently prepared copy request scenarios.

/// The source and destination state required by a copy check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CopyScenario {
    /// A file source and an absent destination.
    Basic,
    /// File copy requiring atomic publication to an absent destination.
    AtomicFile,
    /// File copy requiring durable publication to an absent destination.
    DurableFile,
    /// Atomic copy of a root, `sub` directory, and `sub/child` containing the
    /// supplied bytes.
    AtomicTree,
    /// Durable copy of the same independently prepared two-directory, one-file
    /// tree.
    DurableTree,
    /// A provider-prepared file source and an absent destination requiring
    /// server-side copy. Source contents are supplied by the fixture and
    /// must be independently observable.
    ServerSide,
    /// A file source and a destination containing `b"existing"`.
    Conflict,
}
