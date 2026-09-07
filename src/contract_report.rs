// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Public report of the checks performed by a contract suite.

use qubit_fs::metadata::FileSystemCapability;

use crate::ContractCheck;
use crate::ContractCheckId;
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
    pub(crate) expected: Vec<ContractCheckId>,
    pending: Vec<ContractCheckId>,
    violations: Vec<String>,
}

impl ContractReport {
    /// Returns checks in execution order.
    #[inline]
    pub fn checks(&self) -> &[ContractCheck] {
        &self.checks
    }

    /// Returns whether the registered contract requirements have evidence.
    ///
    /// An unavailable optional probe is acceptable. A probe that ran and failed
    /// is always a failure, even when it was optional.
    #[must_use]
    pub fn requirements_satisfied(&self) -> bool {
        self.requirements_satisfied_with(&[])
    }

    /// Also requires evidence for the caller-selected optional checks.
    ///
    /// A requested identity absent from this run cannot satisfy the request.
    #[must_use]
    pub fn requirements_satisfied_with(&self, required: &[ContractCheckId]) -> bool {
        !self.expected.is_empty()
            && self.pending.is_empty()
            && self.violations.is_empty()
            && self
                .expected
                .iter()
                .all(|id| self.checks.iter().any(|check| check.id() == *id))
            && self.checks.iter().all(|check| match check.outcome() {
                ContractCheckOutcome::Passed | ContractCheckOutcome::RejectedAsExpected => true,
                ContractCheckOutcome::NotApplicable { reason } => !reason.trim().is_empty(),
                ContractCheckOutcome::SkippedOptional { reason } => {
                    !reason.trim().is_empty() && !required.contains(&check.id())
                }
                ContractCheckOutcome::Unverified { .. }
                | ContractCheckOutcome::Failed { .. }
                | ContractCheckOutcome::NotRun { .. } => false,
            })
            && required.iter().all(|id| self.expected.contains(id))
    }

    /// Returns whether every applicable check, including optional probes, was
    /// verified.
    #[must_use]
    pub fn all_applicable_checks_verified(&self) -> bool {
        self.requirements_satisfied_with(&self.expected)
    }

    /// Panics if registered requirements lack successful evidence.
    ///
    /// # Panics
    /// Reports all unverified, failed, missing, or invalidly completed checks.
    #[track_caller]
    pub fn assert_satisfied(&self) {
        self.assert_satisfied_with(&[]);
    }

    /// Also requires the selected optional probes to have evidence.
    ///
    /// # Panics
    /// Panics when any required check or selected optional probe is unverified.
    #[track_caller]
    pub fn assert_satisfied_with(&self, required: &[ContractCheckId]) {
        if self.requirements_satisfied_with(required) {
            return;
        }
        let mut issues = self.violations.clone();
        if self.expected.is_empty() {
            issues.push("report has no registered phase".to_owned());
        }
        for id in &self.expected {
            if !self.checks.iter().any(|check| check.id() == *id) {
                issues.push(format!("{id} (check was not recorded)"));
            }
        }
        for check in &self.checks {
            match check.outcome() {
                ContractCheckOutcome::Unverified { reason }
                | ContractCheckOutcome::Failed { reason }
                | ContractCheckOutcome::NotRun { reason } => issues.push(format!("{} ({reason})", check.id())),
                ContractCheckOutcome::SkippedOptional { reason } if required.contains(&check.id()) => {
                    issues.push(format!("{} (required optional probe: {reason})", check.id()));
                }
                ContractCheckOutcome::SkippedOptional { reason } | ContractCheckOutcome::NotApplicable { reason }
                    if reason.trim().is_empty() =>
                {
                    issues.push(format!("{} (missing applicability reason)", check.id()));
                }
                _ => {}
            }
        }
        for id in required {
            if !self.expected.contains(id) {
                issues.push(format!("{id} (requested probe is outside this run)"));
            }
        }
        panic!(
            "[fs-testkit:report/incomplete] unverified checks: {}",
            issues.join(", ")
        );
    }

    /// Creates an empty report for a suite run.
    #[inline]
    pub(crate) const fn new() -> Self {
        Self {
            checks: Vec::new(),
            expected: Vec::new(),
            pending: Vec::new(),
            violations: Vec::new(),
        }
    }

    /// Registers a catalog entry before a phase starts running.
    pub(crate) fn expect(&mut self, id: ContractCheckId) {
        if !self.expected.contains(&id) {
            self.expected.push(id);
            self.pending.push(id);
        }
    }

    /// Appends one check produced by a suite phase.
    #[inline]
    pub(crate) fn push(
        &mut self,
        id: ContractCheckId,
        capability: Option<FileSystemCapability>,
        outcome: ContractCheckOutcome,
    ) {
        self.checks.push(ContractCheck {
            id,
            capability,
            outcome,
        });
    }

    /// Registers a pending catalog check without completing it.
    pub(crate) fn register(&mut self, id: ContractCheckId, capability: Option<FileSystemCapability>) {
        if self.expected.contains(&id) {
            self.violations.push(format!("{id} (duplicate registration)"));
            return;
        }
        self.expect(id);
        self.push(
            id,
            capability,
            ContractCheckOutcome::NotRun {
                reason: "check not yet executed".to_owned(),
            },
        );
    }

    /// Completes a registered check exactly once, retaining invalid
    /// transitions.
    pub(crate) fn record(
        &mut self,
        id: ContractCheckId,
        capability: Option<FileSystemCapability>,
        outcome: ContractCheckOutcome,
    ) {
        if matches!(outcome, ContractCheckOutcome::SkippedOptional { .. })
            && !crate::internal::check_catalog::specification(id).optional
        {
            self.violations
                .push(format!("{id} (core check cannot be skipped as optional)"));
        }
        let Some(index) = self.pending.iter().position(|pending| *pending == id) else {
            let reason = if self.expected.contains(&id) {
                "duplicate completion"
            } else {
                "unregistered check"
            };
            self.violations.push(format!("{id} ({reason})"));
            return;
        };
        let _ = self.pending.remove(index);
        if let Some(check) = self.checks.iter_mut().find(|check| check.id == id) {
            check.capability = capability;
            check.outcome = outcome;
        } else {
            self.push(id, capability, outcome);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_catalog_entry_is_incomplete() {
        let mut report = ContractReport::new();
        report.expect(ContractCheckId::ReadBasic);
        report.record(ContractCheckId::WriteBasic, None, ContractCheckOutcome::Passed);
        assert!(!report.requirements_satisfied());
    }

    #[test]
    fn unverified_catalog_entry_is_incomplete_until_recorded() {
        let mut report = ContractReport::new();
        report.expect(ContractCheckId::ReadBasic);
        report.record(
            ContractCheckId::ReadBasic,
            None,
            ContractCheckOutcome::Unverified {
                reason: "check not yet executed".to_owned(),
            },
        );

        assert!(!report.requirements_satisfied());
        let result = std::panic::catch_unwind(|| report.assert_satisfied());
        assert!(result.is_err(), "an unexecuted check must fail strict mode");
    }

    #[test]
    fn skipped_optional_does_not_make_report_incomplete() {
        let mut report = ContractReport::new();
        report.expect(ContractCheckId::AsyncCopyCancelReader);
        report.record(
            ContractCheckId::AsyncCopyCancelReader,
            None,
            ContractCheckOutcome::SkippedOptional {
                reason: "probe unavailable".to_owned(),
            },
        );
        assert!(report.requirements_satisfied());
    }

    #[test]
    fn optional_evidence_can_be_required_by_the_caller() {
        let mut report = ContractReport::new();
        let id = ContractCheckId::AsyncCopyCancelReader;
        report.expect(id);
        report.record(
            id,
            None,
            ContractCheckOutcome::SkippedOptional {
                reason: "no suspension gate".to_owned(),
            },
        );
        assert!(report.requirements_satisfied());
        assert!(!report.all_applicable_checks_verified());
        assert!(!report.requirements_satisfied_with(&[id]));
    }

    #[test]
    fn failed_optional_probe_is_never_accepted() {
        let mut report = ContractReport::new();
        report.expect(ContractCheckId::AsyncCopyCancelReader);
        report.record(
            ContractCheckId::AsyncCopyCancelReader,
            None,
            ContractCheckOutcome::Failed {
                reason: "recovery writer was lost".to_owned(),
            },
        );
        assert!(!report.requirements_satisfied());
        assert!(!report.all_applicable_checks_verified());
    }

    #[test]
    fn not_run_is_not_evidence() {
        let mut report = ContractReport::new();
        report.expect(ContractCheckId::ReadBasic);
        report.record(
            ContractCheckId::ReadBasic,
            None,
            ContractCheckOutcome::NotRun {
                reason: "earlier provider panic".to_owned(),
            },
        );
        assert!(!report.requirements_satisfied());
        assert!(!report.all_applicable_checks_verified());
    }

    #[test]
    fn required_probe_must_belong_to_the_report() {
        let mut report = ContractReport::new();
        report.expect(ContractCheckId::ReadBasic);
        report.record(ContractCheckId::ReadBasic, None, ContractCheckOutcome::Passed);
        assert!(report.requirements_satisfied());
        assert!(!report.requirements_satisfied_with(&[ContractCheckId::WriteCancelOpen]));
    }

    #[test]
    fn duplicate_completion_invalidates_report() {
        let mut report = ContractReport::new();
        report.expect(ContractCheckId::ReadBasic);
        report.record(ContractCheckId::ReadBasic, None, ContractCheckOutcome::Passed);
        report.record(ContractCheckId::ReadBasic, None, ContractCheckOutcome::Passed);
        assert!(!report.requirements_satisfied());
    }

    #[test]
    fn unregistered_completion_invalidates_report() {
        let mut report = ContractReport::new();
        report.expect(ContractCheckId::ReadBasic);
        report.record(ContractCheckId::ReadBasic, None, ContractCheckOutcome::Passed);
        report.record(ContractCheckId::WriteBasic, None, ContractCheckOutcome::Passed);
        assert!(!report.requirements_satisfied());
    }

    #[test]
    fn unverified_completion_cannot_be_overwritten_by_success() {
        let mut report = ContractReport::new();
        report.expect(ContractCheckId::ReadBasic);
        report.record(
            ContractCheckId::ReadBasic,
            None,
            ContractCheckOutcome::Unverified {
                reason: "independent read unavailable".to_owned(),
            },
        );
        report.record(ContractCheckId::ReadBasic, None, ContractCheckOutcome::Passed);
        assert!(!report.requirements_satisfied());
    }

    #[test]
    fn core_check_cannot_downgrade_itself_to_optional() {
        let mut report = ContractReport::new();
        report.expect(ContractCheckId::WriteBasic);
        report.record(
            ContractCheckId::WriteBasic,
            None,
            ContractCheckOutcome::SkippedOptional {
                reason: "fixture elected to skip writing".to_owned(),
            },
        );
        assert!(!report.requirements_satisfied());
    }
}
