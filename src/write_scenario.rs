// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Provider-neutral write scenarios with fixed contract expectations.

/// The write behavior whose request a fixture prepares.
#[must_use]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteScenario {
    /// Publish bytes to a previously absent destination.
    Create,
    /// Open a fresh CreateNew writer for explicit abort without committing.
    Abort,
    /// Attempt CreateNew against an independently seeded existing target.
    CreateConflict,
    /// Replace the entire contents of an existing destination.
    /// The initial contents must be longer than the payload to verify
    /// truncation.
    Replace,
    /// Append bytes to an existing destination.
    /// Preparation must supply nonempty initial contents for independent
    /// observation.
    Append,
    /// Write with an explicit absence precondition.
    IfAbsent,
    /// Write with the current destination version as a precondition.
    IfMatch,
    /// Replace an existing destination with required atomicity.
    /// Initial contents must differ from the requested replacement bytes.
    AtomicReplace,
    /// Publish bytes with required durability.
    /// The prepared request uses CreateNew at a previously absent destination.
    Durable,
}
