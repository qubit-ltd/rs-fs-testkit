// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Cleanup evidence retained across explicit retries.

use crate::ContractFailure;

/// Cleanup attempts and their historical failures.
///
/// A later successful teardown does not erase a failed facade deletion.
#[must_use]
pub struct CleanupReport {
    pub(crate) attempts: usize,
    pub(crate) completed: bool,
    pub(crate) failures: Vec<ContractFailure>,
}

impl CleanupReport {
    /// Returns the number of cleanup attempts started, including cancelled
    /// ones.
    #[must_use]
    pub const fn attempts(&self) -> usize {
        self.attempts
    }

    /// Returns whether the latest cleanup attempt returned successfully.
    #[must_use]
    pub const fn completed(&self) -> bool {
        self.completed
    }

    /// Returns every observed failure, including failures before a retry.
    pub fn failures(&self) -> &[ContractFailure] {
        &self.failures
    }

    /// Creates an empty cleanup history.
    pub(crate) const fn new() -> Self {
        Self {
            attempts: 0,
            completed: false,
            failures: Vec::new(),
        }
    }
}
