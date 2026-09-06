// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Public report of the checks performed by a contract suite.

use crate::ContractCheck;
use crate::ContractCheckOutcome;

/// A report containing stable check identities and their outcomes.
#[must_use]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractReport {
    pub(crate) checks: Vec<ContractCheck>,
    /// Check identities registered by the phase catalog.
    ///
    /// Keeping this set separately from `checks` lets completeness detect a
    /// phase whose implementation forgot to record one of its cataloged
    /// checks.  A report that only contains ad-hoc checks must not be able to
    /// claim that a contract phase was fully exercised.
    pub(crate) expected: Vec<&'static str>,
}

impl ContractReport {
    /// Returns checks in execution order.
    #[inline]
    #[must_use]
    pub fn checks(&self) -> &[ContractCheck] {
        &self.checks
    }

    /// Returns whether every recorded check is verified.
    ///
    /// Optional probes that are unavailable do not make a report incomplete.
    #[inline]
    #[must_use]
    pub fn is_complete(&self) -> bool {
        !self.expected.is_empty()
            && self
                .expected
                .iter()
                .all(|id| self.checks.iter().any(|check| check.id() == *id))
            && !self
                .checks
                .iter()
                .any(|check| matches!(check.outcome(), ContractCheckOutcome::Unverified { .. }))
    }

    /// Panics when the report contains an unverified check.
    ///
    /// The panic identifies every incomplete check and preserves its reason so
    /// strict downstream registration cannot silently omit fixture evidence.
    #[track_caller]
    pub fn assert_complete(&self) {
        let mut incomplete = self
            .checks
            .iter()
            .filter_map(|check| match check.outcome() {
                ContractCheckOutcome::Unverified { reason } => {
                    Some(format!("{} ({reason})", check.id()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for id in &self.expected {
            if !self.checks.iter().any(|check| check.id() == *id) {
                incomplete.push(format!("{id} (check was not recorded)"));
            }
        }
        assert!(
            incomplete.is_empty(),
            "[fs-testkit:report/incomplete] unverified checks: {}",
            incomplete.join(", ")
        );
    }

    /// Creates an empty report for a suite run.
    #[inline]
    pub(crate) const fn new() -> Self {
        Self {
            checks: Vec::new(),
            expected: Vec::new(),
        }
    }

    /// Registers a catalog entry before a phase starts running.
    pub(crate) fn expect(&mut self, id: &'static str) {
        if !self.expected.contains(&id) {
            self.expected.push(id);
        }
    }

    /// Appends one check produced by a suite phase.
    #[inline]
    pub(crate) fn push(
        &mut self,
        id: &'static str,
        capability: Option<qubit_fs::metadata::FileSystemCapability>,
        outcome: ContractCheckOutcome,
    ) {
        self.checks.push(ContractCheck {
            id,
            capability,
            outcome,
        });
    }

    /// Replaces the pending entry for a check, or appends a repeated run.
    pub(crate) fn record(
        &mut self,
        id: &'static str,
        capability: Option<qubit_fs::metadata::FileSystemCapability>,
        outcome: ContractCheckOutcome,
    ) {
        if let Some(check) = self.checks.iter_mut().rev().find(|check| {
            check.id == id && matches!(check.outcome, ContractCheckOutcome::Unverified { .. })
        }) {
            check.capability = capability;
            check.outcome = outcome;
            return;
        }
        self.push(id, capability, outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_catalog_entry_is_incomplete() {
        let mut report = ContractReport::new();
        report.expect("cataloged-check");
        report.record(
            "unrelated-check",
            None,
            ContractCheckOutcome::Passed,
        );
        assert!(!report.is_complete());
    }

    #[test]
    fn unverified_catalog_entry_is_incomplete_until_recorded() {
        let mut report = ContractReport::new();
        report.expect("cataloged-check");
        report.record(
            "cataloged-check",
            None,
            ContractCheckOutcome::Unverified {
                reason: "check not yet executed".to_owned(),
            },
        );

        assert!(!report.is_complete());
        let result = std::panic::catch_unwind(|| report.assert_complete());
        assert!(result.is_err(), "an unexecuted check must fail strict mode");
    }

    #[test]
    fn skipped_optional_does_not_make_report_incomplete() {
        let mut report = ContractReport::new();
        report.expect("optional-check");
        report.record(
            "optional-check",
            None,
            ContractCheckOutcome::SkippedOptional {
                reason: "probe unavailable".to_owned(),
            },
        );
        assert!(report.is_complete());
    }
}
