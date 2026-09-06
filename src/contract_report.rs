// qubit-style: allow all
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Public report of the checks performed by a contract suite.

use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::FileSystemCapabilities;

use crate::ContractCheck;
use crate::ContractCheckOutcome;
use crate::FileSystemContract;

/// A report containing stable check identities and their outcomes.
#[must_use]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContractReport {
    pub(crate) checks: Vec<ContractCheck>,
}

impl ContractReport {
    /// Returns checks in execution order.
    #[inline]
    pub fn checks(&self) -> &[ContractCheck] {
        &self.checks
    }

    /// Returns whether every recorded check is verified.
    ///
    /// Optional probes that are unavailable do not make a report incomplete.
    #[inline]
    #[must_use]
    pub fn is_complete(&self) -> bool {
        !self
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
        let incomplete = self
            .checks
            .iter()
            .filter_map(|check| match check.outcome() {
                ContractCheckOutcome::Unverified { reason } => Some(format!("{} ({reason})", check.id())),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            incomplete.is_empty(),
            "[fs-testkit:report/incomplete] unverified checks: {}",
            incomplete.join(", ")
        );
    }

    /// Creates an empty report for a suite run.
    #[inline]
    pub(crate) const fn new() -> Self {
        Self { checks: Vec::new() }
    }

    /// Appends one check produced by a suite phase.
    #[inline]
    pub(crate) fn push(
        &mut self,
        phase: FileSystemContract,
        id: &'static str,
        capability: Option<FileSystemCapability>,
        required: bool,
        outcome: ContractCheckOutcome,
    ) {
        self.checks.push(ContractCheck {
            phase,
            id,
            capability,
            required,
            outcome,
        });
    }

    /// Replaces the pending entry for a check, or appends a repeated run.
    pub(crate) fn record(
        &mut self,
        phase: FileSystemContract,
        id: &'static str,
        capability: Option<FileSystemCapability>,
        required: bool,
        outcome: ContractCheckOutcome,
    ) {
        if let Some(check) = self.checks.iter_mut().rev().find(|check| {
            check.phase == phase && check.id == id && matches!(check.outcome, ContractCheckOutcome::Unverified { .. })
        }) {
            check.capability = capability;
            check.outcome = outcome;
            return;
        }
        self.push(phase, id, capability, required, outcome);
    }

    pub(crate) fn complete_phase(
        &mut self,
        phase: FileSystemContract,
        capabilities: &FileSystemCapabilities,
    ) {
        for check in &mut self.checks {
            if check.phase != phase || !matches!(check.outcome, ContractCheckOutcome::Unverified { .. }) {
                continue;
            }
            if let Some(capability) = check.capability
                && !capabilities.supports(capability)
            {
                check.outcome = if check.required {
                    ContractCheckOutcome::Unverified {
                        reason: format!("provider does not advertise required {capability:?}"),
                    }
                } else {
                    ContractCheckOutcome::SkippedOptional {
                        reason: format!("provider does not advertise {capability:?}"),
                    }
                };
            } else {
                check.outcome = ContractCheckOutcome::Passed;
            }
        }
    }
}
