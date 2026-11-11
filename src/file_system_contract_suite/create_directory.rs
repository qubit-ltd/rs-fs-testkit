// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
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
    ///
    /// # Errors
    ///
    /// Returns a contract failure when either creation mode violates its
    /// contract.
    pub(super) fn check_create_directory(&mut self) -> Result<(), ContractFailure> {
        for id in [ContractCheckId::DirectoryCreate, ContractCheckId::DirectoryRecursive] {
            self.check_create_directory_item(id)?;
        }
        Ok(())
    }

    /// Checks publication and existing-directory handling for one request mode.
    ///
    /// # Parameters
    ///
    /// * `id` - The directory creation check to run; only ordinary and
    ///   recursive creation checks are accepted.
    ///
    /// # Errors
    ///
    /// Returns a contract failure when path preparation, provider I/O, or
    /// verification fails.
    pub(super) fn check_create_directory_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        let recursive = match id {
            ContractCheckId::DirectoryCreate => false,
            ContractCheckId::DirectoryRecursive => true,
            _ => {
                return Err(ContractFailure::message_only("selected check is not directory creation").at(id));
            }
        };
        self.context.begin_check(id);
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
                Ok(_) => {
                    return Err(ContractFailure::message_only("unavailable directory creation succeeded").at(id));
                }
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
            if matches!(before, FixtureSupport::Unsupported) {
                self.context.record_check(
                    id,
                    Some(capability),
                    ContractCheckOutcome::Unverified {
                        reason: "fixture.exists_out_of_band cannot confirm the initial directory premise".to_owned(),
                    },
                );
                return Ok(());
            }
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
            if matches!(observed, FixtureSupport::Unsupported) {
                self.context.record_check(
                    id,
                    Some(capability),
                    ContractCheckOutcome::Unverified {
                        reason: "fixture.exists_out_of_band cannot confirm directory publication".to_owned(),
                    },
                );
                return Ok(());
            }
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
        let mut observation_unsupported = false;
        for path in [&parent, &target] {
            let observed = self.fixture.exists_out_of_band(path).map_err(|error| {
                ContractFailure::with_source("directory rejection precondition observation failed", error).at(id)
            })?;
            if matches!(observed, FixtureSupport::Unsupported) {
                observation_unsupported = true;
                continue;
            }
            verify_condition(
                matches!(observed, FixtureSupport::Supported(true)),
                id,
                "directory rejection precondition path is missing",
            )?;
            let metadata = self.fixture.file_system().stat(path).map_err(|error| {
                ContractFailure::with_source("directory rejection precondition metadata failed", error).at(id)
            })?;
            verify_condition(
                metadata.is_directory_like(),
                id,
                "directory rejection precondition is not directory-like",
            )?;
        }
        let error = match self
            .fixture
            .file_system()
            .create_directory(&target, CreateDirectoryOptions::default().with_recursive(recursive))
        {
            Err(error) => error,
            Ok(_) => {
                return Err(ContractFailure::message_only("existing directory was accepted without exists_ok").at(id));
            }
        };
        let provider_id = self.context.properties().info().provider_id();
        if error.provider() != Some(provider_id) {
            return Err(ContractFailure::with_source(
                "directory rejection error omitted or mismatched provider ID",
                error,
            )
            .at(id));
        }
        verify_fs_error(
            error,
            FsErrorKind::AlreadyExists,
            FsOperation::CreateDir,
            &target,
            provider_id,
            None,
            id,
        )?;
        for path in [&parent, &target] {
            let observed = self.fixture.exists_out_of_band(path).map_err(|error| {
                ContractFailure::with_source("directory rejection observation failed", error).at(id)
            })?;
            if matches!(observed, FixtureSupport::Unsupported) {
                observation_unsupported = true;
                continue;
            }
            verify_condition(
                matches!(observed, FixtureSupport::Supported(true)),
                id,
                "directory rejection removed a requested path",
            )?;
            let metadata =
                self.fixture.file_system().stat(path).map_err(|error| {
                    ContractFailure::with_source("directory rejection metadata failed", error).at(id)
                })?;
            verify_condition(
                metadata.is_directory_like(),
                id,
                "rejected directory is no longer directory-like",
            )?;
        }
        if observation_unsupported {
            self.context.record_check(
                id,
                Some(capability),
                ContractCheckOutcome::Unverified {
                    reason: "fixture.exists_out_of_band cannot confirm directory rejection preserved the paths"
                        .to_owned(),
                },
            );
            return Ok(());
        }
        self.context
            .record_check(id, Some(capability), ContractCheckOutcome::Passed);
        Ok(())
    }
}
