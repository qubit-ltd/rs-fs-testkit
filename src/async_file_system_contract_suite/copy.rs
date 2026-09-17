// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Catalog-driven copy execution with independently prepared check scenarios.

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractFailure;
use crate::FileSystemContract;

impl AsyncFileSystemContractSuite<'_> {
    /// Runs every applicable copy check using the catalog's registered order.
    pub(super) async fn check_copy(&mut self) -> Result<(), ContractFailure> {
        for spec in crate::internal::check_catalog::for_contract(FileSystemContract::Copy, true) {
            self.check_copy_item(spec.id).await?;
        }
        Ok(())
    }

    /// Executes only the selected copy requirement.
    pub(super) async fn check_copy_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        match id {
            ContractCheckId::CopyBasic => self.check_basic_copy(id).await,
            ContractCheckId::CopyFallbackOverwriteRejected => self.check_copy_conflict().await,
            ContractCheckId::CopyServerSide => self.check_server_side_copy().await,
            ContractCheckId::CopyAtomicFile | ContractCheckId::CopyDurableFile => self.check_strong_file_copy(id).await,
            ContractCheckId::CopyAtomicTree | ContractCheckId::CopyDurableTree => self.check_strong_tree_copy(id).await,
            ContractCheckId::CopyRepeatedExecute => self.check_basic_copy(id).await,
            ContractCheckId::AsyncCopyCancelNativeAttempt
            | ContractCheckId::AsyncCopyCancelReader
            | ContractCheckId::AsyncCopyCancelWriter
            | ContractCheckId::AsyncCopyCancelCommit => self.check_copy_cancellation_item(id).await,
            _ => Err(ContractFailure::message_only("selected check is not supported by this copy driver").at(id)),
        }
    }
}
