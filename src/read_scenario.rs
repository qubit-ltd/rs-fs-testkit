// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Independently prepared read scenarios with suite-owned expectations.

/// Identifies the initial state needed by one read check.
///
/// The fixture prepares a path and the supplied content through an independent
/// channel. The suite constructs the request and checks its fixed semantics.
#[must_use]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadScenario {
    /// Read complete bytes and enforce the caller's byte budget.
    Basic,
    /// Read a bounded window from the supplied content.
    Range,
    /// Check admission at and above the declared range limit.
    RangeLimit,
    /// Read using the independently observed current version.
    IfMatchCurrent,
    /// Reject a version independently known to be stale.
    IfMatchStale,
    /// Reject exclusion of the independently observed current version.
    IfNoneMatchCurrent,
    /// Read when the excluded version is independently known to be stale.
    IfNoneMatchStale,
    /// Read intact content with required checksum validation.
    Checksum,
    /// Reject independently corrupted content with required validation.
    ChecksumCorruption,
}
