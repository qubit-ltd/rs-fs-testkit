// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Dispatches temporary-resource checks through the typed catalog.

use super::*;
use crate::ContractCheckId;

impl<'a> AsyncFileSystemContractSuite<'a> {
    /// Executes all asynchronous temporary-resource checks in catalog order.
    pub(super) async fn check_temp_resources(&mut self) -> Result<(), crate::ContractFailure> {
        for id in [
            ContractCheckId::TempFile,
            ContractCheckId::TempDirectory,
            ContractCheckId::TempAtomic,
            ContractCheckId::TempRepeatedLifecycle,
        ] {
            self.check_temp_item(id).await?;
        }
        Ok(())
    }

    /// Executes exactly one temporary-resource catalog entry.
    pub(super) async fn check_temp_item(&mut self, id: ContractCheckId) -> Result<(), crate::ContractFailure> {
        self.context.begin(id.as_str());
        let file = self.capable(FileSystemCapability::TempFile);
        let directory = self.capable(FileSystemCapability::TempDirectory);
        let outcome = match id {
            ContractCheckId::TempFile => {
                self.execute_temp_file(id).await?;
                if file {
                    ContractCheckOutcome::Passed
                } else {
                    ContractCheckOutcome::RejectedAsExpected
                }
            }
            ContractCheckId::TempDirectory => {
                self.execute_temp_directory(id).await?;
                if directory {
                    ContractCheckOutcome::Passed
                } else {
                    ContractCheckOutcome::RejectedAsExpected
                }
            }
            ContractCheckId::TempAtomic | ContractCheckId::TempRepeatedLifecycle => {
                if file {
                    self.execute_temp_file(id).await?;
                }
                if directory {
                    self.execute_temp_directory(id).await?;
                }
                if !file && !directory {
                    ContractCheckOutcome::NotApplicable {
                        reason: "no temporary resource capability".to_owned(),
                    }
                } else if id == ContractCheckId::TempAtomic && !self.capable(FileSystemCapability::AtomicTempPersist) {
                    ContractCheckOutcome::RejectedAsExpected
                } else {
                    ContractCheckOutcome::Passed
                }
            }
            _ => return Err(crate::ContractFailure::message_only("selected entry is not temporary lifecycle").at(id)),
        };
        self.context.record_check(
            id,
            crate::internal::check_catalog::specification(id).capability,
            outcome,
        );
        Ok(())
    }
}
