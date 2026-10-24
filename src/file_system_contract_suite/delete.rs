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

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::DeleteScenario;
use crate::FileSystemContractSuite;
use crate::FixturePreparation;
use crate::FixtureSupport;
use crate::internal::verify_condition;
use crate::internal::verify_fs_error;

impl FileSystemContractSuite<'_> {
    /// Runs the basic, missing-target, and conditional file deletion checks.
    ///
    /// Each check prepares its own target so one scenario cannot affect
    /// another. Returns the first fixture or contract failure encountered.
    ///
    /// # Errors
    ///
    /// Returns a [`ContractFailure`] when preparation or observation fails,
    /// or when the file system violates a deletion contract.
    pub(super) fn check_delete(&mut self) -> Result<(), ContractFailure> {
        for id in [
            ContractCheckId::DeleteBasic,
            ContractCheckId::DeleteMissingOk,
            ContractCheckId::DeleteIfMatch,
        ] {
            self.check_delete_item(id)?;
        }
        Ok(())
    }

    /// Records one deletion check while preserving its original failure.
    ///
    /// # Parameters
    ///
    /// * `id` identifies the deletion contract to execute. `DeleteTree` is
    ///   delegated to the recursive deletion check; other accepted values
    ///   select one of the file deletion scenarios.
    ///
    /// # Errors
    ///
    /// Returns a [`ContractFailure`] if scenario execution or fixture
    /// observation fails.
    pub(super) fn check_delete_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        self.context.begin(id.as_str());
        if id == ContractCheckId::DeleteTree {
            return self.check_delete_tree();
        }
        let capability = if id == ContractCheckId::DeleteIfMatch {
            FileSystemCapability::ConditionalDelete
        } else {
            FileSystemCapability::Delete
        };
        let outcome = self.execute_delete(id)?;
        self.context.record_check(id, Some(capability), outcome);
        Ok(())
    }

    /// Executes one file deletion contract using an independently prepared
    /// target and records the observed outcome.
    ///
    /// Capability rejection, missing-target behavior, and conditional
    /// deletion each have separate handling; prepared targets are verified
    /// before and after deletion. Returns an unverified outcome when required
    /// fixture support is unavailable.
    ///
    /// # Errors
    ///
    /// Returns a [`ContractFailure`] if fixture preparation, file-system
    /// operations, or contract observations fail.
    fn execute_delete(&mut self, id: ContractCheckId) -> Result<ContractCheckOutcome, ContractFailure> {
        let relative = self.context.relative_name("delete-target");
        if !self.capable(FileSystemCapability::Delete) {
            return self.reject_unavailable_delete(id, &relative);
        }
        if id == ContractCheckId::DeleteMissingOk {
            return self.check_missing_delete(id, &relative);
        }
        if id == ContractCheckId::DeleteIfMatch && !self.capable(FileSystemCapability::ConditionalDelete) {
            return self.reject_unavailable_conditional_delete(id, &relative);
        }
        let scenario = match id {
            ContractCheckId::DeleteBasic => DeleteScenario::Basic,
            ContractCheckId::DeleteIfMatch => DeleteScenario::IfMatch,
            _ => {
                return Err(ContractFailure::message_only("selected check is not file deletion").at(id));
            }
        };
        self.execute_prepared_delete(id, scenario, &relative)
    }

    /// Confirms that unsupported deletion capabilities are rejected as
    /// required by the basic deletion contract.
    ///
    /// # Errors
    ///
    /// Returns a [`ContractFailure`] if the rejection has an unexpected
    /// error kind or the fixture path cannot be prepared.
    fn reject_unavailable_delete(
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
        let error = match self.fixture.file_system().delete_file(&path, DeleteOptions::default()) {
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

    /// Verifies that deleting an absent target with `missing_ok` succeeds
    /// without creating a resource.
    ///
    /// # Errors
    ///
    /// Returns a [`ContractFailure`] if the target is not initially absent,
    /// deletion fails, or the final observation violates the contract.
    fn check_missing_delete(
        &mut self,
        id: ContractCheckId,
        relative: &str,
    ) -> Result<ContractCheckOutcome, ContractFailure> {
        let path = self
            .fixture
            .path(relative)
            .map_err(|error| ContractFailure::with_source("missing-delete path preparation failed", error).at(id))?;
        let before = self
            .fixture
            .exists_out_of_band(&path)
            .map_err(|error| ContractFailure::with_source("missing-delete initial observation failed", error).at(id))?;
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
            .map_err(|error| ContractFailure::with_source("missing-ok deletion failed", error).at(id))?;
        verify_condition(
            outcome.already_missing(),
            id,
            "missing-ok outcome did not report absence",
        )?;
        let after = self
            .fixture
            .exists_out_of_band(&path)
            .map_err(|error| ContractFailure::with_source("missing-delete final observation failed", error).at(id))?;
        verify_condition(
            matches!(after, FixtureSupport::Supported(false)),
            id,
            "missing-ok deletion created a resource",
        )?;
        Ok(ContractCheckOutcome::Passed)
    }

    /// Confirms that conditional deletion without advertised support is
    /// rejected with a requirement error.
    ///
    /// # Errors
    ///
    /// Returns a [`ContractFailure`] if the path cannot be prepared or the
    /// operation does not report the expected rejection.
    fn reject_unavailable_conditional_delete(
        &mut self,
        id: ContractCheckId,
        relative: &str,
    ) -> Result<ContractCheckOutcome, ContractFailure> {
        let path = self.fixture.path(relative).map_err(|error| {
            ContractFailure::with_source("conditional delete path preparation failed", error).at(id)
        })?;
        let options = DeleteOptions::default().with_if_match(Some(ResourceVersion::new("contract-version")));
        let error = match self.fixture.file_system().delete_file(&path, options) {
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
        Ok(ContractCheckOutcome::RejectedAsExpected)
    }

    /// Prepares, verifies, and removes a file for a supported delete scenario.
    ///
    /// Conditional deletion first proves stale versions are rejected and the
    /// current version remains removable.
    ///
    /// # Errors
    ///
    /// Returns a [`ContractFailure`] if fixture support or file-system behavior
    /// violates the selected scenario.
    fn execute_prepared_delete(
        &mut self,
        id: ContractCheckId,
        scenario: DeleteScenario,
        relative: &str,
    ) -> Result<ContractCheckOutcome, ContractFailure> {
        let bytes = b"delete evidence";
        let prepared = self
            .fixture
            .prepare_delete(scenario, relative, bytes)
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
            .map_err(|error| ContractFailure::with_source("delete initial observation failed", error).at(id))?;
        verify_condition(
            matches!(initial, FixtureSupport::Supported(actual) if actual == bytes),
            id,
            "delete seed lacks independently confirmed contents",
        )?;
        let mut options = DeleteOptions::default();
        if id == ContractCheckId::DeleteIfMatch {
            let current = self.fixture.resource_version(&path).map_err(|error| {
                ContractFailure::with_source("delete current version observation failed", error).at(id)
            })?;
            let stale = self.fixture.stale_resource_version(&path).map_err(|error| {
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
            let retained = self.fixture.read_file(&path).map_err(|error| {
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
            .map_err(|error| ContractFailure::with_source("delete publication observation failed", error).at(id))?;
        verify_condition(
            matches!(after, FixtureSupport::Supported(false)),
            id,
            "delete/basic: deleted file remained",
        )?;
        Ok(ContractCheckOutcome::Passed)
    }
}
