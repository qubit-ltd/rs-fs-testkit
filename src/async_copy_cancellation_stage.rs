// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cancellation stages exercised by asynchronous copy contract probes.

/// Cancellation point exercised by an asynchronous copy contract probe.
#[must_use]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AsyncCopyCancellationStage {
    /// The provider-native copy attempt is pending.
    NativeAttempt,
    /// The fallback source reader is pending.
    Reader,
    /// The fallback target writer is pending.
    Writer,
    /// The fallback commit is pending.
    Commit,
}
