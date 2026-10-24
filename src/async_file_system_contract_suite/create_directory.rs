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
use qubit_fs::path::Path;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixtureSupport;
use crate::internal::verify_condition;
use crate::internal::verify_fs_error;

impl AsyncFileSystemContractSuite<'_> {
    /// Runs ordinary and recursive directory creation contracts independently.
    ///
    /// Each check receives a separately derived target name. The selected
    /// operation is rejected with an unsupported-capability error when the
    /// provider reports that directory creation is unavailable; otherwise,
    /// the check verifies publication and acceptance of an existing directory.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after both checks have been recorded.
    ///
    /// # Errors
    ///
    /// Returns a contract failure if either target cannot be prepared, an
    /// unsupported operation is accepted, required absence cannot be
    /// confirmed, creation or metadata observation fails, or the provider
    /// reports an incorrect creation result.
    pub(super) async fn check_create_directory(&mut self) -> Result<(), ContractFailure> {
        for id in [ContractCheckId::DirectoryCreate, ContractCheckId::DirectoryRecursive] {
            self.check_create_directory_item(id).await?;
        }
        Ok(())
    }

    /// Checks one ordinary or recursive directory creation request.
    ///
    /// The target and its possible ancestor must be independently confirmed
    /// absent before creation. Recursive creation also requires a positive
    /// created-ancestor count when the provider supplies that count. After
    /// creation, each requested path must be independently visible and
    /// directory-like, and a repeated request with `exists_ok` enabled must
    /// report that the target already existed. If the capability is
    /// unavailable, the request must be rejected with the matching filesystem
    /// error instead.
    ///
    /// # Parameters
    ///
    /// * `id` - Directory creation check to execute.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after the selected check has been recorded.
    ///
    /// # Errors
    ///
    /// Returns a contract failure if `id` is not a directory creation check,
    /// if an initial absence or later publication cannot be independently
    /// observed, or if the provider's capability, creation, metadata, or
    /// existing-directory result violates the selected contract.
    pub(super) async fn check_create_directory_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        let recursive = match id {
            ContractCheckId::DirectoryCreate => false,
            ContractCheckId::DirectoryRecursive => true,
            _ => {
                return Err(ContractFailure::message_only("selected check is not directory creation").at(id));
            }
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
            return self
                .check_create_directory_rejected(id, capability, &target, options)
                .await;
        }
        self.verify_create_directory_targets_absent(id, &parent, &target)
            .await?;
        self.context.record_created(parent.clone());
        self.context.record_created(target.clone());
        let outcome = self
            .fixture
            .file_system()
            .create_directory(&target, options)
            .await
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
        self.verify_create_directory_publication(id, &parent, &target).await?;
        self.verify_create_directory_exists_ok(id, recursive, &target).await?;
        self.context
            .record_check(id, Some(capability), ContractCheckOutcome::Passed);
        Ok(())
    }

    /// Verifies that a provider rejects a directory request for an unavailable
    /// capability.
    ///
    /// # Parameters
    ///
    /// * `id` - Contract check used to associate any failure with the request.
    /// * `capability` - Capability expected to be unavailable.
    /// * `target` - Target path supplied to the provider.
    /// * `options` - Ordinary or recursive request options under examination.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after recording the expected capability rejection.
    ///
    /// # Errors
    ///
    /// Returns a contract failure if the operation succeeds or its error does
    /// not identify the expected unsupported create-directory operation.
    async fn check_create_directory_rejected(
        &mut self,
        id: ContractCheckId,
        capability: FileSystemCapability,
        target: &Path,
        options: CreateDirectoryOptions,
    ) -> Result<(), ContractFailure> {
        let error = match self.fixture.file_system().create_directory(target, options).await {
            Err(error) => error,
            Ok(_) => {
                return Err(ContractFailure::message_only("unavailable directory creation succeeded").at(id));
            }
        };
        verify_fs_error(
            error,
            FsErrorKind::UnsupportedCapability,
            FsOperation::CreateDir,
            target,
            self.context.properties().info().provider_id(),
            Some(capability),
            id,
        )?;
        self.context
            .record_check(id, Some(capability), ContractCheckOutcome::RejectedAsExpected);
        Ok(())
    }

    /// Confirms the recursive ancestor and requested target are initially
    /// absent.
    ///
    /// # Parameters
    ///
    /// * `id` - Contract check used to associate observation failures.
    /// * `parent` - Path that recursive creation may need to create.
    /// * `target` - Requested directory path.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` when both paths are confirmed absent.
    ///
    /// # Errors
    ///
    /// Returns a contract failure if either path cannot be independently
    /// observed as absent.
    async fn verify_create_directory_targets_absent(
        &self,
        id: ContractCheckId,
        parent: &Path,
        target: &Path,
    ) -> Result<(), ContractFailure> {
        for path in [parent, target] {
            let before =
                self.fixture.exists_out_of_band(path).await.map_err(|error| {
                    ContractFailure::with_source("directory initial observation failed", error).at(id)
                })?;
            verify_condition(
                matches!(before, FixtureSupport::Supported(false)),
                id,
                "directory fixture needs independently confirmed absent paths",
            )?;
        }
        Ok(())
    }

    /// Checks independent publication and directory metadata for requested
    /// paths.
    ///
    /// # Parameters
    ///
    /// * `id` - Contract check used to associate observation failures.
    /// * `parent` - Possible ancestor whose publication recursive mode
    ///   requires.
    /// * `target` - Requested directory path.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` when both paths are published as directories.
    ///
    /// # Errors
    ///
    /// Returns a contract failure if a path is not observed as present or its
    /// metadata is not directory-like.
    async fn verify_create_directory_publication(
        &self,
        id: ContractCheckId,
        parent: &Path,
        target: &Path,
    ) -> Result<(), ContractFailure> {
        for path in [parent, target] {
            let observed = self.fixture.exists_out_of_band(path).await.map_err(|error| {
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
                .await
                .map_err(|error| ContractFailure::with_source("created directory metadata failed", error).at(id))?;
            verify_condition(
                metadata.is_directory_like(),
                id,
                "created resource is not directory-like",
            )?;
        }
        Ok(())
    }

    /// Repeats creation with `exists_ok` and checks the already-existing
    /// result.
    ///
    /// # Parameters
    ///
    /// * `id` - Contract check used to associate the provider response.
    /// * `recursive` - Whether the repeated request includes recursive mode.
    /// * `target` - Directory already created by the preceding contract phase.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` when the provider reports the target already existed.
    ///
    /// # Errors
    ///
    /// Returns a contract failure if the provider rejects the existing target
    /// or does not report that it already existed.
    async fn verify_create_directory_exists_ok(
        &self,
        id: ContractCheckId,
        recursive: bool,
        target: &Path,
    ) -> Result<(), ContractFailure> {
        let outcome = self
            .fixture
            .file_system()
            .create_directory(
                target,
                CreateDirectoryOptions::default()
                    .with_exists_ok(true)
                    .with_recursive(recursive),
            )
            .await
            .map_err(|error| ContractFailure::with_source("existing directory was not accepted", error).at(id))?;
        verify_condition(
            outcome.already_existed(),
            id,
            "existing directory outcome was not reported",
        )?;
        Ok(())
    }
}
