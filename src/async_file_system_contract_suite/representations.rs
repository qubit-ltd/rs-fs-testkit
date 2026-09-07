// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Independent empty-directory and symbolic-link representation evidence.

use qubit_fs::metadata::FileKind;
use qubit_fs::metadata::FileSystemCapability;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl AsyncFileSystemContractSuite<'_> {
    /// Executes representation entries without sharing fixture preparation.
    pub(super) async fn check_representations(&mut self) -> Result<(), ContractFailure> {
        for id in [
            ContractCheckId::RepresentationEmpty,
            ContractCheckId::RepresentationSymlink,
        ] {
            self.check_representation_item(id).await?;
        }
        Ok(())
    }

    /// Requires a real seeded representation for every advertised capability.
    pub(super) async fn check_representation_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        let (capability, name) = match id {
            ContractCheckId::RepresentationEmpty => (FileSystemCapability::EmptyDirectory, "empty-directory"),
            ContractCheckId::RepresentationSymlink => (FileSystemCapability::Symlink, "symlink"),
            _ => return Err(ContractFailure::message_only("selected entry is not a representation check").at(id)),
        };
        self.context.begin(id.as_str());
        if !self.capable(capability) {
            self.context.record_check(
                id,
                Some(capability),
                ContractCheckOutcome::NotApplicable {
                    reason: "provider does not advertise this representation; no rejection was exercised".to_owned(),
                },
            );
            return Ok(());
        }
        let relative = self.context.relative_name(name);
        let prepared = if id == ContractCheckId::RepresentationEmpty {
            self.fixture.seed_empty_directory(&relative).await
        } else {
            self.fixture.seed_symlink(&relative).await
        }
        .map_err(|error| ContractFailure::with_source("representation preparation failed", error).at(id))?;
        let path = match prepared {
            FixtureSupport::Supported(path) => path,
            FixtureSupport::Unsupported => {
                self.context.record_check(
                    id,
                    Some(capability),
                    ContractCheckOutcome::Unverified {
                        reason: "advertised representation requires independent fixture preparation".to_owned(),
                    },
                );
                return Ok(());
            }
        };
        self.context.record_created(path.clone());
        let metadata = self.fixture.file_system().stat(&path).await.map_err(|error| {
            ContractFailure::with_source("representation metadata observation failed", error).at(id)
        })?;
        if id == ContractCheckId::RepresentationEmpty {
            verify_condition(
                metadata.is_directory_like(),
                id,
                "representation contract: empty directory is not directory-like",
            )?;
        } else {
            verify_condition(
                metadata.kind() == &FileKind::Symlink,
                id,
                "representation contract: seeded link kind mismatch",
            )?;
        }
        self.context
            .record_check(id, Some(capability), ContractCheckOutcome::Passed);
        Ok(())
    }
}
