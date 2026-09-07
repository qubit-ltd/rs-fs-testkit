// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Catalog-driven copy execution with independently prepared check scenarios.

use crate::ContractCheckId;
use crate::ContractFailure;
use crate::FileSystemContract;
use crate::FileSystemContractSuite;

impl FileSystemContractSuite<'_> {
    /// Runs every applicable copy check using the catalog's registered order.
    pub(super) fn check_copy(&mut self) -> Result<(), ContractFailure> {
        for spec in crate::internal::check_catalog::for_contract(FileSystemContract::Copy, false) {
            self.check_copy_item(spec.id)?;
        }
        Ok(())
    }

    /// Executes only the selected copy requirement.
    pub(super) fn check_copy_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        match id {
            ContractCheckId::CopyBasic => self.check_basic_copy(id),
            ContractCheckId::CopyFallbackOverwriteRejected => self.check_copy_conflict(),
            ContractCheckId::CopyServerSide => self.check_server_side_copy(),
            ContractCheckId::CopyAtomicFile | ContractCheckId::CopyDurableFile => self.check_strong_file_copy(id),
            ContractCheckId::CopyAtomicTree | ContractCheckId::CopyDurableTree => self.check_strong_tree_copy(id),
            _ => Err(ContractFailure::message_only("selected check is not supported by this copy driver").at(id)),
        }
    }
}
