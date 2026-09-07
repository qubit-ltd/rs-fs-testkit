// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Independent missing-path and seeded metadata evidence.

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixtureSupport;
use crate::internal::verify_condition;
use crate::internal::verify_fs_error;

impl AsyncFileSystemContractSuite<'_> {
    /// Executes both metadata entries through their independent drivers.
    pub(super) async fn check_stat(&mut self) -> Result<(), ContractFailure> {
        for id in [ContractCheckId::StatBasic, ContractCheckId::StatFileKind] {
            self.check_stat_item(id).await?;
        }
        Ok(())
    }

    /// Records exactly the selected metadata entry.
    pub(super) async fn check_stat_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        self.context.begin(id.as_str());
        let spec = crate::internal::check_catalog::specification(id);
        let outcome =
            match id {
                ContractCheckId::StatBasic => {
                    let relative = self.context.relative_name("stat-missing");
                    let path = self.fixture.path(&relative).map_err(|error| {
                        ContractFailure::with_source("stat missing path preparation failed", error).at(id)
                    })?;
                    let error = match self.fixture.file_system().stat(&path).await {
                        Err(error) => error,
                        Ok(_) => return Err(ContractFailure::message_only("stat missing path succeeded").at(id)),
                    };
                    verify_fs_error(
                        error,
                        FsErrorKind::NotFound,
                        FsOperation::Stat,
                        &path,
                        self.context.properties().info().provider_id(),
                        None,
                        id,
                    )?;
                    ContractCheckOutcome::RejectedAsExpected
                }
                ContractCheckId::StatFileKind => {
                    let bytes = b"stateful stat";
                    let relative = self.context.relative_name("stat-file");
                    let prepared =
                        self.fixture.seed_file(&relative, bytes).await.map_err(|error| {
                            ContractFailure::with_source("stat file preparation failed", error).at(id)
                        })?;
                    match prepared {
                        FixtureSupport::Supported(path) => {
                            self.context.record_created(path.clone());
                            let metadata = self.fixture.file_system().stat(&path).await.map_err(|error| {
                                ContractFailure::with_source("seeded file stat failed", error).at(id)
                            })?;
                            verify_condition(
                                metadata.is_file_like(),
                                id,
                                "stat/file-kind: seeded resource is not file-like",
                            )?;
                            verify_condition(
                                metadata.len() == Some(bytes.len() as u64),
                                id,
                                "stat file length mismatch",
                            )?;
                            ContractCheckOutcome::Passed
                        }
                        FixtureSupport::Unsupported => ContractCheckOutcome::Unverified {
                            reason: "fixture cannot seed a stat resource".to_owned(),
                        },
                    }
                }
                _ => return Err(ContractFailure::message_only("selected entry is not a metadata check").at(id)),
            };
        self.context.record_check(id, spec.capability, outcome);
        Ok(())
    }
}
