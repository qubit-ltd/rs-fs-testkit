// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Recursive removal observed independently at every prepared depth.

use qubit_fs::directory::DeleteOptions;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixtureSupport;
use crate::internal::verify_condition;
use crate::internal::verify_fs_error;

impl AsyncFileSystemContractSuite<'_> {
    /// Prepares the tree through fixture channels, never through the tested
    /// facade.
    pub(super) async fn check_delete_tree(&mut self) -> Result<(), ContractFailure> {
        let id = ContractCheckId::DeleteTree;
        self.context.begin(id.as_str());
        let capability = FileSystemCapability::RecursiveDelete;
        let relative = self.context.relative_name("recursive-delete-root");
        let options = DeleteOptions::default().with_recursive(true);
        if !self.capable(capability) {
            let root = self.fixture.path(&relative).map_err(|error| {
                ContractFailure::with_source("recursive delete path preparation failed", error).at(id)
            })?;
            let error = match self.fixture.file_system().delete_directory(&root, options).await {
                Err(error) => error,
                Ok(_) => return Err(ContractFailure::message_only("unavailable recursive deletion succeeded").at(id)),
            };
            verify_fs_error(
                error,
                FsErrorKind::RequirementNotMet,
                FsOperation::Delete,
                &root,
                self.context.properties().info().provider_id(),
                Some(capability),
                id,
            )?;
            self.context
                .record_check(id, Some(capability), ContractCheckOutcome::RejectedAsExpected);
            return Ok(());
        }
        let mut paths = Vec::new();
        for directory in [relative.clone(), format!("{relative}/nested")] {
            let prepared = self.fixture.seed_empty_directory(&directory).await.map_err(|error| {
                ContractFailure::with_source("recursive-delete directory preparation failed", error).at(id)
            })?;
            let FixtureSupport::Supported(path) = prepared else {
                self.context.record_check(
                    id,
                    Some(capability),
                    ContractCheckOutcome::Unverified {
                        reason: "recursive deletion requires independently prepared directories".to_owned(),
                    },
                );
                return Ok(());
            };
            self.context.record_created(path.clone());
            paths.push(path);
        }
        for file in [format!("{relative}/child"), format!("{relative}/nested/child")] {
            let prepared = self.fixture.seed_file(&file, b"child").await.map_err(|error| {
                ContractFailure::with_source("recursive-delete child preparation failed", error).at(id)
            })?;
            let FixtureSupport::Supported(path) = prepared else {
                self.context.record_check(
                    id,
                    Some(capability),
                    ContractCheckOutcome::Unverified {
                        reason: "recursive deletion requires independently prepared children".to_owned(),
                    },
                );
                return Ok(());
            };
            self.context.record_created(path.clone());
            paths.push(path);
        }
        for path in &paths {
            let before = self.fixture.exists_out_of_band(path).await.map_err(|error| {
                ContractFailure::with_source("recursive-delete initial observation failed", error).at(id)
            })?;
            verify_condition(
                matches!(before, FixtureSupport::Supported(true)),
                id,
                "recursive-delete seed must independently contain every entry",
            )?;
        }
        let outcome = self
            .fixture
            .file_system()
            .delete_directory(&paths[0], options)
            .await
            .map_err(|error| ContractFailure::with_source("recursive removal failed", error).at(id))?;
        verify_condition(
            !outcome.already_missing(),
            id,
            "recursive deletion reported a prepared tree missing",
        )?;
        for path in &paths {
            let after = self.fixture.exists_out_of_band(path).await.map_err(|error| {
                ContractFailure::with_source("recursive-delete final observation failed", error).at(id)
            })?;
            verify_condition(
                matches!(after, FixtureSupport::Supported(false)),
                id,
                "delete/tree: child remained after removal or root remained after removal",
            )?;
        }
        self.context
            .record_check(id, Some(capability), ContractCheckOutcome::Passed);
        Ok(())
    }
}
