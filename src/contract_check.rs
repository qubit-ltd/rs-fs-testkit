// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Stable identity and outcome for one contract check.

use qubit_fs::metadata::FileSystemCapability;

use crate::ContractCheckOutcome;
use crate::FileSystemContract;

/// One named check in a contract report.
#[must_use]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractCheck {
    pub(crate) phase: FileSystemContract,
    pub(crate) id: &'static str,
    pub(crate) capability: Option<FileSystemCapability>,
    pub(crate) required: bool,
    pub(crate) outcome: ContractCheckOutcome,
}

impl ContractCheck {
    /// Returns the contract phase that produced this check.
    #[inline]
    #[must_use]
    pub const fn phase(&self) -> FileSystemContract {
        self.phase
    }

    /// Returns the stable check identifier.
    #[inline]
    #[must_use]
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// Returns the capability associated with this check, when one exists.
    #[inline]
    #[must_use]
    pub const fn capability(&self) -> Option<FileSystemCapability> {
        self.capability
    }

    /// Returns whether this check is required for a complete report.
    #[inline]
    #[must_use]
    pub const fn is_required(&self) -> bool {
        self.required
    }

    /// Returns the recorded check outcome.
    #[inline]
    #[must_use]
    pub const fn outcome(&self) -> &ContractCheckOutcome {
        &self.outcome
    }
}
