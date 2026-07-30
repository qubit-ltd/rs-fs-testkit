// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Typed availability results for provider-owned fixture probes.

/// Distinguishes a supported fixture probe from one the fixture cannot offer.
///
/// # Type Parameters
///
/// * `T` - Value returned when the fixture supports the probe.
#[must_use]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FixtureSupport<T> {
    /// The fixture supplied the requested provider-specific value.
    Supported(T),
    /// The fixture cannot supply this optional probe.
    Unsupported,
}
