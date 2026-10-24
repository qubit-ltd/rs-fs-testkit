// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cleanup evidence retained across explicit retries.

use crate::ContractFailure;

/// Cleanup attempts and their historical failures.
///
/// A later successful teardown does not erase a failed facade deletion.
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
/// assert!(suite.run().cleanup().completed());
/// ```
#[must_use]
pub struct CleanupReport {
    /// Number of cleanup attempts that started, including cancelled attempts.
    pub(crate) attempts: usize,
    /// Whether the latest cleanup attempt returned successfully.
    pub(crate) completed: bool,
    /// Failures observed across all cleanup attempts, in observation order.
    pub(crate) failures: Vec<ContractFailure>,
}

impl CleanupReport {
    /// Creates an empty cleanup history.
    pub(crate) const fn new() -> Self {
        Self {
            attempts: 0,
            completed: false,
            failures: Vec::new(),
        }
    }

    /// Returns the number of cleanup attempts started, including cancelled
    /// ones.
    ///
    /// # Returns
    /// The number of attempts that began before this report was observed.
    #[must_use = "inspect the retained cleanup failures"]
    #[inline]
    pub const fn attempts(&self) -> usize {
        self.attempts
    }

    /// Returns whether the latest cleanup attempt returned successfully.
    ///
    /// # Returns
    /// `true` when the latest attempt returned successfully, and `false` when
    /// it failed or was cancelled.
    #[must_use = "inspect the retained cleanup failures"]
    #[inline]
    pub const fn completed(&self) -> bool {
        self.completed
    }

    /// Returns every observed failure, including failures before a retry.
    ///
    /// # Returns
    /// The failures from all attempts in the order they were observed.
    #[must_use = "inspect the retained cleanup failures"]
    #[inline]
    pub fn failures(&self) -> &[ContractFailure] {
        &self.failures
    }
}
