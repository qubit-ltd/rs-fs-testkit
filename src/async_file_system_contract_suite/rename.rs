// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Independently prepared rename publication and conflict evidence.

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::AchievedAtomicity;
use qubit_fs::metadata::AtomicityRequirement;
use qubit_fs::metadata::DurabilityRequirement;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::rename::RenameFailureState;
use qubit_fs::rename::RenameOptions;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl AsyncFileSystemContractSuite<'_> {
    /// Runs each rename requirement against independently prepared paths.
    pub(super) async fn check_rename(&mut self) -> Result<(), ContractFailure> {
        for id in [
            ContractCheckId::RenameBasic,
            ContractCheckId::RenameConflict,
            ContractCheckId::RenameAtomic,
            ContractCheckId::RenameDurable,
        ] {
            self.check_rename_item(id).await?;
        }
        Ok(())
    }

    /// Records only the selected requirement and retains original rename
    /// errors.
    pub(super) async fn check_rename_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        self.context.begin(id.as_str());
        let (capability, mut options) = match id {
            ContractCheckId::RenameBasic | ContractCheckId::RenameConflict => {
                (FileSystemCapability::Rename, RenameOptions::default())
            }
            ContractCheckId::RenameAtomic => (
                FileSystemCapability::AtomicRename,
                RenameOptions::default().with_atomicity(AtomicityRequirement::Required),
            ),
            ContractCheckId::RenameDurable => (
                FileSystemCapability::DurableRename,
                RenameOptions::default().with_durability(DurabilityRequirement::Required),
            ),
            _ => return Err(ContractFailure::message_only("selected entry is not a rename check").at(id)),
        };
        if id == ContractCheckId::RenameConflict && !self.capable(capability) {
            self.context.record_check(
                id,
                Some(capability),
                ContractCheckOutcome::NotApplicable {
                    reason: "Rename capability is unavailable".to_owned(),
                },
            );
            return Ok(());
        }
        let source_relative = self.context.relative_name("rename-source");
        let target_relative = self.context.relative_name("rename-target");
        if !self.capable(capability) {
            let source = self
                .fixture
                .path(&source_relative)
                .map_err(|error| ContractFailure::with_source("rename source path preparation failed", error).at(id))?;
            let target = self
                .fixture
                .path(&target_relative)
                .map_err(|error| ContractFailure::with_source("rename target path preparation failed", error).at(id))?;
            let failure = match self.fixture.file_system().rename(&source, &target, options).await {
                Err(failure) => failure,
                Ok(_) => return Err(ContractFailure::message_only("unavailable rename requirement succeeded").at(id)),
            };
            let kind = if capability == FileSystemCapability::Rename {
                FsErrorKind::UnsupportedCapability
            } else {
                FsErrorKind::RequirementNotMet
            };
            let error = failure.error();
            if error.kind() != kind
                || error.operation() != FsOperation::Rename
                || error.path() != Some(&source)
                || error.target() != Some(&target)
                || error.provider() != Some(self.context.properties().info().provider_id())
                || error.required_capability() != Some(capability)
                || failure.state() != RenameFailureState::Unchanged
            {
                return Err(ContractFailure::with_source("rename preflight rejection differs", failure).at(id));
            }
            self.context
                .record_check(id, Some(capability), ContractCheckOutcome::RejectedAsExpected);
            return Ok(());
        }
        let bytes = b"rename source";
        let prepared = self
            .fixture
            .seed_file(&source_relative, bytes)
            .await
            .map_err(|error| ContractFailure::with_source("rename source preparation failed", error).at(id))?;
        let FixtureSupport::Supported(source) = prepared else {
            self.context.record_check(
                id,
                Some(capability),
                ContractCheckOutcome::Unverified {
                    reason: "rename needs independently prepared source contents".to_owned(),
                },
            );
            return Ok(());
        };
        self.context.record_created(source.clone());
        let target = if id == ContractCheckId::RenameConflict {
            let prepared = self
                .fixture
                .seed_file(&target_relative, b"old target")
                .await
                .map_err(|error| {
                    ContractFailure::with_source("rename conflict target preparation failed", error).at(id)
                })?;
            let FixtureSupport::Supported(target) = prepared else {
                self.context.record_check(
                    id,
                    Some(capability),
                    ContractCheckOutcome::Unverified {
                        reason: "rename conflict needs an independently prepared target".to_owned(),
                    },
                );
                return Ok(());
            };
            target
        } else {
            self.fixture
                .path(&target_relative)
                .map_err(|error| ContractFailure::with_source("rename target preparation failed", error).at(id))?
        };
        self.context.record_created(target.clone());
        verify_condition(source != target, id, "rename fixture requires distinct paths")?;
        let initial =
            self.fixture.read_file(&source).await.map_err(|error| {
                ContractFailure::with_source("rename initial source observation failed", error).at(id)
            })?;
        verify_condition(
            matches!(initial, FixtureSupport::Supported(actual) if actual == bytes),
            id,
            "rename source fixture contents differ",
        )?;
        if id == ContractCheckId::RenameConflict {
            let initial_target = self.fixture.read_file(&target).await.map_err(|error| {
                ContractFailure::with_source("rename initial target observation failed", error).at(id)
            })?;
            verify_condition(
                matches!(initial_target, FixtureSupport::Supported(actual) if actual == b"old target"),
                id,
                "rename conflict target fixture contents differ",
            )?;
            let failure = match self
                .fixture
                .file_system()
                .rename(&source, &target, options.clone())
                .await
            {
                Err(failure) => failure,
                Ok(_) => {
                    return Err(
                        ContractFailure::message_only("rename contract: default conflict replaced target").at(id),
                    );
                }
            };
            let error = failure.error();
            if failure.state() != RenameFailureState::Unchanged
                || error.kind() != FsErrorKind::AlreadyExists
                || error.operation() != FsOperation::Rename
                || error.path() != Some(&source)
                || error.target() != Some(&target)
                || error.provider() != Some(self.context.properties().info().provider_id())
            {
                return Err(ContractFailure::with_source("rename conflict rejection differs", failure).at(id));
            }
            for (path, expected) in [(&source, bytes.as_slice()), (&target, b"old target".as_slice())] {
                let observed = self.fixture.read_file(path).await.map_err(|error| {
                    ContractFailure::with_source("rename conflict observation failed", error).at(id)
                })?;
                verify_condition(
                    matches!(observed, FixtureSupport::Supported(actual) if actual == expected),
                    id,
                    "rename conflict changed source or target",
                )?;
            }
            options = options.with_overwrite(true);
        } else {
            let before = self.fixture.exists_out_of_band(&target).await.map_err(|error| {
                ContractFailure::with_source("rename initial target observation failed", error).at(id)
            })?;
            verify_condition(
                matches!(before, FixtureSupport::Supported(false)),
                id,
                "rename target must initially be absent",
            )?;
        }
        let outcome = self
            .fixture
            .file_system()
            .rename(&source, &target, options)
            .await
            .map_err(|error| ContractFailure::with_source("rename/basic: rename failed", error).at(id))?;
        verify_condition(
            outcome.source() == &source && outcome.target() == &target,
            id,
            "rename/basic: source context mismatch or target context mismatch",
        )?;
        if id == ContractCheckId::RenameAtomic {
            verify_condition(
                outcome.atomicity() == AchievedAtomicity::Atomic,
                id,
                "rename/atomic: required operation reported non-atomic publication",
            )?;
        }
        if id == ContractCheckId::RenameDurable {
            verify_condition(
                outcome.durable(),
                id,
                "rename/durable: required operation reported non-durable publication",
            )?;
        }
        let exists =
            self.fixture.exists_out_of_band(&source).await.map_err(|error| {
                ContractFailure::with_source("rename source removal observation failed", error).at(id)
            })?;
        verify_condition(
            matches!(exists, FixtureSupport::Supported(false)),
            id,
            "rename/basic: source remained after success",
        )?;
        let observed =
            self.fixture.read_file(&target).await.map_err(|error| {
                ContractFailure::with_source("rename target content observation failed", error).at(id)
            })?;
        verify_condition(
            matches!(observed, FixtureSupport::Supported(actual) if actual == bytes),
            id,
            "rename/basic: target bytes mismatch",
        )?;
        self.context
            .record_check(id, Some(capability), ContractCheckOutcome::Passed);
        Ok(())
    }
}
