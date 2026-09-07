// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Independent file preparation for deletion checks.

/// Identifies the file state required by a deletion check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeleteScenario {
    /// An existing file whose removal can be independently observed.
    Basic,
    /// An existing file with independent current and stale version
    /// observations.
    IfMatch,
}
