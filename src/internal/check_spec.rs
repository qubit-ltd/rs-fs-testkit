// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! One immutable entry in the contract execution catalog.

use qubit_fs::metadata::FileSystemCapability;

use crate::ContractCheckId;
use crate::FileSystemContract;

/// Execution ownership and applicability shared by both I/O drivers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CheckSpec {
    /// Stable typed identity.
    pub(crate) id: ContractCheckId,
    /// Sole owning phase.
    pub(crate) contract: FileSystemContract,
    /// Capability whose claim this check verifies.
    pub(crate) capability: Option<FileSystemCapability>,
    /// Typed read preparation, when this is a read check.
    pub(crate) read_scenario: Option<crate::ReadScenario>,
    /// Typed write preparation; cancellation instead uses its stage probe.
    pub(crate) write_scenario: Option<crate::WriteScenario>,
    /// Typed copy preparation; cancellation uses its stage probe.
    pub(crate) copy_scenario: Option<crate::CopyScenario>,
    /// Whether the check requires an asynchronous facade.
    pub(crate) asynchronous_only: bool,
    /// Whether unavailable fixture instrumentation may be skipped.
    pub(crate) optional: bool,
}

impl CheckSpec {
    /// Returns checks whose setup is a prerequisite for this evidence item.
    ///
    /// Prerequisites describe evidence ordering only; `run_check` still runs
    /// exactly the requested identity and never silently executes siblings.
    #[allow(dead_code)]
    pub(crate) const fn prerequisites(self) -> &'static [ContractCheckId] {
        match self.id {
            ContractCheckId::CopyRepeatedExecute
            | ContractCheckId::CopyFallbackOverwriteRejected
            | ContractCheckId::CopyServerSide
            | ContractCheckId::CopyAtomicFile
            | ContractCheckId::CopyAtomicTree
            | ContractCheckId::CopyDurableFile
            | ContractCheckId::CopyDurableTree => &[ContractCheckId::CopyBasic],
            ContractCheckId::WriteRepeatedExecute => &[ContractCheckId::WriteBasic],
            ContractCheckId::TempAtomic | ContractCheckId::TempRepeatedLifecycle => {
                &[ContractCheckId::TempFile, ContractCheckId::TempDirectory]
            }
            _ => &[],
        }
    }
}
