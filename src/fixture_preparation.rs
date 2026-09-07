// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Typed preparation evidence supplied by an isolated provider fixture.

/// Separates a prepared request from applicability and missing instrumentation.
///
/// Preparation failures use `FixtureResult::Err`. A fixture cannot turn a
/// backend error into an inapplicability declaration. The suite validates
/// declarations against the provider's capability guarantees.
#[must_use]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FixturePreparation<T> {
    /// The request and independent initial state are ready to exercise.
    Ready(T),
    /// The scenario does not exist in this provider configuration.
    NotApplicable {
        /// Concrete provider restriction that excludes this scenario.
        reason: String,
    },
    /// The scenario applies, but fixture evidence is unavailable.
    Unavailable {
        /// The missing setup or observation needed to perform the check.
        reason: String,
    },
}
