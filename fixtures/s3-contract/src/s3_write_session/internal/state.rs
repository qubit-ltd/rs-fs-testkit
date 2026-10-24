// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

/// Tracks whether the buffered write has been published or may have been published.
pub(super) enum State {
    /// The session accepts bytes or can retry after a non-publication failure.
    Open,
    /// The PUT completed successfully.
    Published,
    /// A PUT may have reached the store, but its outcome is unknown.
    Indeterminate,
}
