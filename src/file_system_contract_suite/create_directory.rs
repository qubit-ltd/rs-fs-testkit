// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Independently prepared ordinary and recursive directory creation.

use qubit_fs::directory::CreateDirectoryOptions;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FileSystemContractSuite;
use crate::FixtureSupport;
use crate::internal::verify_condition;
use crate::internal::verify_fs_error;

impl FileSystemContractSuite<'_> {
    /// Runs each directory creation check with its own fresh target.
    pub(super) fn check_create_directory(&mut self) -> Result<(), ContractFailure> {
        for id in [ContractCheckId::DirectoryCreate, ContractCheckId::DirectoryRecursive] {
            self.check_create_directory_item(id)?;
        }
        Ok(())
    }

    /// Checks publication and existing-directory handling for one request mode.
    pub(super) fn check_create_directory_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        let recursive = match id {
            ContractCheckId::DirectoryCreate => false,
            ContractCheckId::DirectoryRecursive => true,
            _ => return Err(ContractFailure::message_only("selected check is not directory creation").at(id)),
        };
        self.context.begin(id.as_str());
        let relative = self.context.relative_name("created-directory");
        let parent = self
            .fixture
            .path(&relative)
            .map_err(|error| ContractFailure::with_source("directory path preparation failed", error).at(id))?;
        let target = if recursive {
            self.fixture.path(&format!("{relative}/child")).map_err(|error| {
                ContractFailure::with_source("recursive directory path preparation failed", error).at(id)
            })?
        } else {
            parent.clone()
        };
        let capability = FileSystemCapability::CreateDirectory;
        let options = CreateDirectoryOptions::default().with_recursive(recursive);
        if !self.capable(capability) {
            let error = match self.fixture.file_system().create_directory(&target, options) {
                Err(error) => error,
                Ok(_) => return Err(ContractFailure::message_only("unavailable directory creation succeeded").at(id)),
            };
            verify_fs_error(
                error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::CreateDir,
                &target,
                self.context.properties().info().provider_id(),
                Some(capability),
                id,
            )?;
            self.context
                .record_check(id, Some(capability), ContractCheckOutcome::RejectedAsExpected);
            return Ok(());
        }
        for path in [&parent, &target] {
            let before = self
                .fixture
                .exists_out_of_band(path)
                .map_err(|error| ContractFailure::with_source("directory initial observation failed", error).at(id))?;
            verify_condition(
                matches!(before, FixtureSupport::Supported(false)),
                id,
                "directory fixture needs independently confirmed absent paths",
            )?;
        }
        self.context.record_created(parent.clone());
        self.context.record_created(target.clone());
        let outcome = self
            .fixture
            .file_system()
            .create_directory(&target, options)
            .map_err(|error| ContractFailure::with_source("directory creation failed", error).at(id))?;
        verify_condition(
            !outcome.already_existed(),
            id,
            "new directory reported as already existing",
        )?;
        if recursive {
            verify_condition(
                outcome.created_ancestors().is_none_or(|count| count > 0),
                id,
                "recursive ancestor count is zero",
            )?;
        }
        for path in [&parent, &target] {
            let observed = self.fixture.exists_out_of_band(path).map_err(|error| {
                ContractFailure::with_source("directory publication observation failed", error).at(id)
            })?;
            verify_condition(
                matches!(observed, FixtureSupport::Supported(true)),
                id,
                "directory creation did not publish every requested ancestor and target",
            )?;
            let metadata = self
                .fixture
                .file_system()
                .stat(path)
                .map_err(|error| ContractFailure::with_source("created directory metadata failed", error).at(id))?;
            verify_condition(
                metadata.is_directory_like(),
                id,
                "created resource is not directory-like",
            )?;
        }
        let outcome = self
            .fixture
            .file_system()
            .create_directory(
                &target,
                CreateDirectoryOptions::default()
                    .with_exists_ok(true)
                    .with_recursive(recursive),
            )
            .map_err(|error| ContractFailure::with_source("existing directory was not accepted", error).at(id))?;
        verify_condition(
            outcome.already_existed(),
            id,
            "existing directory outcome was not reported",
        )?;
        self.context
            .record_check(id, Some(capability), ContractCheckOutcome::Passed);
        Ok(())
    }
}
