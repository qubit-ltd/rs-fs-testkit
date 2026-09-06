// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Outcomes recorded by filesystem contract checks.

/// The observable result of one stable contract check.
#[must_use]
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractCheckOutcome {
    /// The provider satisfied the check.
    Passed,
    /// The provider rejected a request that the contract requires it to reject.
    RejectedAsExpected,
    /// The check does not apply to this fixture or provider configuration.
    NotApplicable {
        /// Why this provider or fixture does not expose the scenario.
        reason: String,
    },
    /// The check could not be verified from the fixture's available evidence.
    Unverified {
        /// Why the suite could not produce independent evidence.
        reason: String,
    },
    /// An optional diagnostic probe was unavailable.
    SkippedOptional {
        /// Why an optional diagnostic probe was unavailable.
        reason: String,
    },
}
