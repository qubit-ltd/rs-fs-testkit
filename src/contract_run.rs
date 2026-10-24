// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Results owned by a single filesystem contract execution session.

use crate::ContractFailure;
use crate::ContractReport;

/// Retains contract evidence and execution diagnostics in the borrowing suite.
///
/// A completed run may fail its requirements. Completion means only that
/// execution and its automatic cleanup attempt returned.
///
/// # Examples
///
/// ```
/// # mod support { include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/common/rustdoc_support.rs")); }
/// # use support::RustdocSyncFixture;
/// use qubit_fs_testkit::{FileSystemContract, FileSystemContractSuite};
///
/// let fixture = RustdocSyncFixture::new();
/// let mut suite = FileSystemContractSuite::new(&fixture);
/// suite.run_contract(FileSystemContract::Properties);
/// let run = suite.run();
/// assert!(!run.was_interrupted());
/// assert!(!run.report().checks().is_empty());
/// ```
#[must_use]
pub struct ContractRun {
    /// Checks and evidence recorded during the contract execution.
    pub(crate) report: ContractReport,
    /// Cleanup completion and failures retained for this run.
    pub(crate) cleanup: crate::CleanupReport,
    /// Execution failures retained in observation order.
    pub(crate) failures: Vec<ContractFailure>,
    /// Whether execution started and may have been interrupted.
    pub(crate) started: bool,
    /// Whether execution returned and completed its cleanup attempt.
    pub(crate) completed: bool,
}

impl ContractRun {
    /// Creates the evidence container before any provider operation runs.
    pub(crate) const fn new() -> Self {
        Self {
            report: ContractReport::new(),
            cleanup: crate::CleanupReport::new(),
            failures: Vec::new(),
            started: false,
            completed: false,
        }
    }

    /// Returns the registered checks and their evidence.
    ///
    /// # Returns
    ///
    /// The report containing every check and its evidence.
    #[must_use = "inspect the contract report"]
    #[inline]
    pub const fn report(&self) -> &ContractReport {
        &self.report
    }

    /// Returns cleanup evidence and the history of all attempts.
    ///
    /// # Returns
    ///
    /// The cleanup report retained by this run.
    #[must_use = "inspect the cleanup report"]
    #[inline]
    pub const fn cleanup(&self) -> &crate::CleanupReport {
        &self.cleanup
    }

    /// Returns preserved failures in observation order.
    ///
    /// # Returns
    ///
    /// The failures recorded while executing the contract.
    #[inline]
    pub fn failures(&self) -> &[ContractFailure] {
        &self.failures
    }

    /// Reports that a started session was dropped before returning its result.
    ///
    /// The runner exclusively borrows the suite while active. Callers can
    /// inspect this retained state only after that borrow ends, including when
    /// a pending execution or cleanup future is cancelled.
    ///
    /// # Returns
    ///
    /// `true` when the session started but did not complete.
    #[must_use]
    #[inline]
    pub const fn was_interrupted(&self) -> bool {
        self.started && !self.completed
    }

    /// Returns whether execution ended and every required check has evidence.
    ///
    /// # Returns
    ///
    /// `true` when execution, required evidence, and cleanup all succeeded.
    #[must_use]
    pub fn requirements_satisfied(&self) -> bool {
        self.requirements_satisfied_with(&[])
    }

    /// Also requires evidence for caller-selected optional probes.
    ///
    /// Missing selections outside this run fail rather than being ignored.
    ///
    /// # Parameters
    ///
    /// * `required` - Optional check identities that must have evidence.
    ///
    /// # Returns
    ///
    /// `true` when execution and cleanup succeeded and every selected check
    /// has evidence.
    #[must_use]
    pub fn requirements_satisfied_with(&self, required: &[crate::ContractCheckId]) -> bool {
        self.execution_satisfied() && self.report.requirements_satisfied_with(required)
    }

    /// Includes every applicable optional probe in the evidence requirement.
    ///
    /// # Returns
    ///
    /// `true` when execution and cleanup succeeded and all applicable checks
    /// have evidence.
    #[must_use]
    pub fn all_applicable_checks_verified(&self) -> bool {
        self.execution_satisfied() && self.report.all_applicable_checks_verified()
    }

    /// Asserts the run's requirements and prints retained safe diagnostics.
    ///
    /// # Panics
    /// Panics if execution, contract evidence, or cleanup did not succeed.
    #[track_caller]
    pub fn assert_satisfied(&self) {
        self.assert_satisfied_with(&[]);
    }

    /// Asserts the run including caller-selected optional probes.
    ///
    /// # Panics
    /// Panics for execution or cleanup failures and missing required evidence.
    ///
    /// # Parameters
    ///
    /// * `required` - Optional check identities that must have evidence.
    #[track_caller]
    pub fn assert_satisfied_with(&self, required: &[crate::ContractCheckId]) {
        let diagnostics = self
            .failures
            .iter()
            .chain(self.cleanup.failures().iter())
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        assert!(
            self.execution_satisfied(),
            "[fs-testkit:run] completed={} cleanup_completed={}: {diagnostics}",
            self.completed,
            self.cleanup.completed()
        );
        self.report.assert_satisfied_with(required);
    }

    /// Keeps cleanup and lifecycle requirements identical for every policy.
    #[must_use]
    fn execution_satisfied(&self) -> bool {
        self.completed && self.failures.is_empty() && self.cleanup.completed() && self.cleanup.failures().is_empty()
    }
}
