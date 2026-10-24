// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Independent basic, missing-target and conditional deletion evidence.

use qubit_fs::directory::DeleteOptions;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::ResourceVersion;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::DeleteScenario;
use crate::FixturePreparation;
use crate::FixtureSupport;
use crate::internal::verify_condition;
use crate::internal::verify_fs_error;

impl AsyncFileSystemContractSuite<'_> {
    /// Runs the basic, missing-target, and conditional file-deletion checks.
    ///
    /// Each scenario is prepared and recorded independently so one failure is
    /// attributed to its own contract check.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after all deletion checks pass.
    ///
    /// # Errors
    ///
    /// Returns the first preparation, filesystem, or contract-verification
    /// failure, tagged with the check that produced it.
    pub(super) async fn check_delete(&mut self) -> Result<(), ContractFailure> {
        for id in [
            ContractCheckId::DeleteBasic,
            ContractCheckId::DeleteMissingOk,
            ContractCheckId::DeleteIfMatch,
        ] {
            self.check_delete_item(id).await?;
        }
        Ok(())
    }

    /// Records exactly one deletion check and preserves its original failure.
    ///
    /// A tree-deletion identifier is delegated to the tree contract; other
    /// identifiers execute a file-deletion scenario and record its capability
    /// and outcome.
    ///
    /// # Parameters
    ///
    /// * `id` — identifies the deletion contract scenario to execute.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after recording the check, or delegates tree deletion
    /// to its contract check.
    ///
    /// # Errors
    ///
    /// Returns a preparation, filesystem, or verification failure with `id`
    /// attached to its diagnostic context.
    pub(super) async fn check_delete_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        self.context.begin(id.as_str());
        if id == ContractCheckId::DeleteTree {
            return self.check_delete_tree().await;
        }
        let capability = if id == ContractCheckId::DeleteIfMatch {
            FileSystemCapability::ConditionalDelete
        } else {
            FileSystemCapability::Delete
        };
        let outcome = self.execute_delete(id).await?;
        self.context.record_check(id, Some(capability), outcome);
        Ok(())
    }

    /// Selects the file-deletion scenario handler and returns its outcome.
    ///
    /// Capability rejection, missing-target behavior, and seeded file
    /// deletion are handled separately so each helper owns one contract path.
    ///
    /// # Parameters
    ///
    /// * `id` — identifies the file-deletion contract scenario.
    ///
    /// # Returns
    ///
    /// The recorded outcome, or an unverified/not-applicable outcome when the
    /// fixture or required capability cannot support the scenario.
    ///
    /// # Errors
    ///
    /// Returns preparation, filesystem, or contract-verification failures
    /// tagged with `id`.
    async fn execute_delete(&mut self, id: ContractCheckId) -> Result<ContractCheckOutcome, ContractFailure> {
        let relative = self.context.relative_name("delete-target");
        if !self.capable(FileSystemCapability::Delete) {
            return self.execute_unavailable_delete(id, &relative).await;
        }
        if id == ContractCheckId::DeleteMissingOk {
            return self.execute_missing_delete(id, &relative).await;
        }
        if id == ContractCheckId::DeleteIfMatch && !self.capable(FileSystemCapability::ConditionalDelete) {
            let path = self.fixture.path(&relative).map_err(|error| {
                ContractFailure::with_source("conditional delete path preparation failed", error).at(id)
            })?;
            let options = DeleteOptions::default().with_if_match(Some(ResourceVersion::new("contract-version")));
            let error = match self.fixture.file_system().delete_file(&path, options).await {
                Err(error) => error,
                Ok(_) => {
                    return Err(ContractFailure::message_only("unavailable conditional deletion succeeded").at(id));
                }
            };
            verify_fs_error(
                error,
                FsErrorKind::RequirementNotMet,
                FsOperation::Delete,
                &path,
                self.context.properties().info().provider_id(),
                Some(FileSystemCapability::ConditionalDelete),
                id,
            )?;
            return Ok(ContractCheckOutcome::RejectedAsExpected);
        }
        self.execute_seeded_delete(id, &relative).await
    }

    /// Verifies that a filesystem without delete support rejects a basic
    /// delete.
    ///
    /// Other deletion checks require that capability and are reported as not
    /// applicable without attempting an operation.
    ///
    /// # Parameters
    ///
    /// * `id` — identifies the check whose capability is unavailable.
    /// * `relative` — fixture-relative name used to resolve the target path.
    ///
    /// # Returns
    ///
    /// The expected rejection outcome, or not-applicable for non-basic checks.
    ///
    /// # Errors
    ///
    /// Returns path preparation or filesystem-contract verification failures.
    async fn execute_unavailable_delete(
        &mut self,
        id: ContractCheckId,
        relative: &str,
    ) -> Result<ContractCheckOutcome, ContractFailure> {
        if id != ContractCheckId::DeleteBasic {
            return Ok(ContractCheckOutcome::NotApplicable {
                reason: "Delete capability is unavailable".to_owned(),
            });
        }
        let path = self
            .fixture
            .path(relative)
            .map_err(|error| ContractFailure::with_source("delete path preparation failed", error).at(id))?;
        let error = match self
            .fixture
            .file_system()
            .delete_file(&path, DeleteOptions::default())
            .await
        {
            Err(error) => error,
            Ok(_) => {
                return Err(ContractFailure::message_only("unavailable deletion succeeded").at(id));
            }
        };
        verify_fs_error(
            error,
            FsErrorKind::UnsupportedCapability,
            FsOperation::Delete,
            &path,
            self.context.properties().info().provider_id(),
            Some(FileSystemCapability::Delete),
            id,
        )?;
        Ok(ContractCheckOutcome::RejectedAsExpected)
    }

    /// Confirms that deleting an absent target with `missing_ok` leaves it
    /// absent.
    ///
    /// The target is independently checked before and after the operation so
    /// the reported outcome reflects the filesystem state.
    ///
    /// # Parameters
    ///
    /// * `id` — identifies the missing-target contract check.
    /// * `relative` — fixture-relative name used to resolve the absent target.
    ///
    /// # Returns
    ///
    /// `Passed` after the operation reports the target missing and the target
    /// remains absent.
    ///
    /// # Errors
    ///
    /// Returns path preparation, filesystem, observation, or verification
    /// failures tagged with `id`.
    async fn execute_missing_delete(
        &mut self,
        id: ContractCheckId,
        relative: &str,
    ) -> Result<ContractCheckOutcome, ContractFailure> {
        let path = self
            .fixture
            .path(relative)
            .map_err(|error| ContractFailure::with_source("missing-delete path preparation failed", error).at(id))?;
        let before =
            self.fixture.exists_out_of_band(&path).await.map_err(|error| {
                ContractFailure::with_source("missing-delete initial observation failed", error).at(id)
            })?;
        verify_condition(
            matches!(before, FixtureSupport::Supported(false)),
            id,
            "missing-delete fixture target must be absent",
        )?;
        self.context.record_created(path.clone());
        let outcome = self
            .fixture
            .file_system()
            .delete_file(&path, DeleteOptions::default().with_missing_ok(true))
            .await
            .map_err(|error| ContractFailure::with_source("missing-ok deletion failed", error).at(id))?;
        verify_condition(
            outcome.already_missing(),
            id,
            "missing-ok outcome did not report absence",
        )?;
        let after =
            self.fixture.exists_out_of_band(&path).await.map_err(|error| {
                ContractFailure::with_source("missing-delete final observation failed", error).at(id)
            })?;
        verify_condition(
            matches!(after, FixtureSupport::Supported(false)),
            id,
            "missing-ok deletion created a resource",
        )?;
        Ok(ContractCheckOutcome::Passed)
    }

    /// Deletes a prepared file and verifies conditional-delete semantics.
    ///
    /// For conditional checks this first proves a stale version is rejected
    /// without changing the seeded file, then deletes with the current version.
    /// Basic checks use the same independently observed seed and final absence.
    ///
    /// # Parameters
    ///
    /// * `id` — identifies either the basic or conditional deletion check.
    /// * `relative` — fixture-relative name supplied to scenario preparation.
    ///
    /// # Returns
    ///
    /// The deletion outcome, or `Unverified` when fixture preparation or
    /// version observations cannot support the requested contract.
    ///
    /// # Errors
    ///
    /// Returns preparation, filesystem, or contract-verification failures
    /// tagged with `id`.
    async fn execute_seeded_delete(
        &mut self,
        id: ContractCheckId,
        relative: &str,
    ) -> Result<ContractCheckOutcome, ContractFailure> {
        let scenario = match id {
            ContractCheckId::DeleteBasic => DeleteScenario::Basic,
            ContractCheckId::DeleteIfMatch => DeleteScenario::IfMatch,
            _ => {
                return Err(ContractFailure::message_only("selected check is not file deletion").at(id));
            }
        };
        let bytes = b"delete evidence";
        let prepared = self
            .fixture
            .prepare_delete(scenario, relative, bytes)
            .await
            .map_err(|error| ContractFailure::with_source("delete scenario preparation failed", error).at(id))?;
        let path = match prepared {
            FixturePreparation::Ready(path) => path,
            FixturePreparation::Unavailable { reason } | FixturePreparation::NotApplicable { reason } => {
                return Ok(ContractCheckOutcome::Unverified { reason });
            }
        };
        self.context.record_created(path.clone());
        let initial = self
            .fixture
            .read_file(&path)
            .await
            .map_err(|error| ContractFailure::with_source("delete initial observation failed", error).at(id))?;
        verify_condition(
            matches!(initial, FixtureSupport::Supported(actual) if actual == bytes),
            id,
            "delete seed lacks independently confirmed contents",
        )?;
        let mut options = DeleteOptions::default();
        if id == ContractCheckId::DeleteIfMatch {
            let current = self.fixture.resource_version(&path).await.map_err(|error| {
                ContractFailure::with_source("delete current version observation failed", error).at(id)
            })?;
            let stale = self.fixture.stale_resource_version(&path).await.map_err(|error| {
                ContractFailure::with_source("delete stale version observation failed", error).at(id)
            })?;
            let (FixtureSupport::Supported(current), FixtureSupport::Supported(stale)) = (current, stale) else {
                return Ok(ContractCheckOutcome::Unverified {
                    reason: "conditional delete needs current and stale versions".to_owned(),
                });
            };
            verify_condition(current != stale, id, "stale delete version equals the current version")?;
            let error = match self
                .fixture
                .file_system()
                .delete_file(&path, DeleteOptions::default().with_if_match(Some(stale)))
                .await
            {
                Err(error) => error,
                Ok(_) => {
                    return Err(ContractFailure::message_only("delete/if-match: stale version was accepted").at(id));
                }
            };
            verify_fs_error(
                error,
                FsErrorKind::PreconditionFailed,
                FsOperation::Delete,
                &path,
                self.context.properties().info().provider_id(),
                None,
                id,
            )?;
            let retained = self.fixture.read_file(&path).await.map_err(|error| {
                ContractFailure::with_source("stale delete content observation failed", error).at(id)
            })?;
            verify_condition(
                matches!(retained, FixtureSupport::Supported(actual) if actual == bytes),
                id,
                "stale delete changed target contents",
            )?;
            options = options.with_if_match(Some(current));
        }
        let outcome = self
            .fixture
            .file_system()
            .delete_file(&path, options)
            .await
            .map_err(|error| ContractFailure::with_source("delete/basic: deletion failed", error).at(id))?;
        verify_condition(
            !outcome.already_missing(),
            id,
            "delete/basic: existing file was reported missing",
        )?;
        verify_condition(
            outcome.deleted_entries().is_none_or(|count| count > 0),
            id,
            "delete/basic: deleted count is zero",
        )?;
        let after = self
            .fixture
            .exists_out_of_band(&path)
            .await
            .map_err(|error| ContractFailure::with_source("delete publication observation failed", error).at(id))?;
        verify_condition(
            matches!(after, FixtureSupport::Supported(false)),
            id,
            "delete/basic: deleted file remained",
        )?;
        Ok(ContractCheckOutcome::Passed)
    }
}
