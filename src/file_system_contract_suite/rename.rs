// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Independently prepared rename publication and conflict evidence.

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::AchievedAtomicity;
use qubit_fs::metadata::AtomicityRequirement;
use qubit_fs::metadata::DurabilityRequirement;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::path::Path;
use qubit_fs::rename::RenameFailureState;
use qubit_fs::rename::RenameOptions;
use qubit_fs::rename::RenameOutcome;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FileSystemContractSuite;
use crate::FixtureSupport;
use crate::internal::is_expected_unavailable_rename;
use crate::internal::verify_condition;

impl FileSystemContractSuite<'_> {
    /// Runs each rename requirement against independently prepared paths.
    ///
    /// # Returns
    /// Returns `Ok(())` after every rename check has been recorded.
    ///
    /// # Errors
    /// Returns the first failure to prepare fixtures or verify a rename
    /// requirement.
    pub(super) fn check_rename(&mut self) -> Result<(), ContractFailure> {
        for id in [
            ContractCheckId::RenameBasic,
            ContractCheckId::RenameConflict,
            ContractCheckId::RenameAtomic,
            ContractCheckId::RenameDurable,
        ] {
            self.check_rename_item(id)?;
        }
        Ok(())
    }

    /// Records only the selected requirement and retains original rename
    /// errors.
    ///
    /// # Parameters
    /// * `id` - Rename check to execute and record.
    ///
    /// # Returns
    /// Returns `Ok(())` after recording the selected check outcome.
    ///
    /// # Errors
    /// Returns a failure when `id` is not a rename check, fixture operations
    /// fail, or the provider violates the selected rename requirement.
    pub(super) fn check_rename_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        self.context.begin_check(id);
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
            _ => {
                return Err(ContractFailure::message_only("selected entry is not a rename check").at(id));
            }
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
            verify_condition(source != target, id, "rename fixture requires distinct paths")?;
            // Build an independent baseline before the facade preflight. A
            // successful seed is registered immediately so failures in later
            // hooks still leave it in the cleanup ledger.
            let seed = self
                .fixture
                .seed_file(&source_relative, b"rename rejection source")
                .map_err(|error| {
                    ContractFailure::with_source("rename rejection source preparation failed", error).at(id)
                })?;
            let mut source_expected_absent = false;
            let baseline = match seed {
                FixtureSupport::Supported(path) => {
                    self.context.record_created(path.clone());
                    verify_condition(path == source, id, "rename rejection source path differs")?;
                    match self.fixture.read_file(&source).map_err(|error| {
                        ContractFailure::with_source("rename rejection baseline observation failed", error).at(id)
                    })? {
                        FixtureSupport::Supported(bytes) if bytes == b"rename rejection source" => Some(bytes),
                        FixtureSupport::Unsupported => None,
                        FixtureSupport::Supported(_) => {
                            return Err(
                                ContractFailure::message_only("rename rejection source baseline differs").at(id),
                            );
                        }
                    }
                }
                FixtureSupport::Unsupported => {
                    source_expected_absent = true;
                    None
                }
            };
            let mut unverifiable = baseline.is_none() && !source_expected_absent;
            if source_expected_absent {
                for path in [&source, &target] {
                    match self.fixture.exists_out_of_band(path).map_err(|error| {
                        ContractFailure::with_source("rename rejection absence baseline failed", error).at(id)
                    })? {
                        FixtureSupport::Supported(false) => {}
                        FixtureSupport::Supported(true) => {
                            return Err(ContractFailure::message_only(
                                "rename rejection absence baseline was not empty",
                            )
                            .at(id));
                        }
                        FixtureSupport::Unsupported => {
                            // Continue to validate the facade rejection; the
                            // missing baseline will keep the result unverified.
                            unverifiable = true;
                        }
                    }
                }
            }
            if !source_expected_absent {
                match self.fixture.exists_out_of_band(&target).map_err(|error| {
                    ContractFailure::with_source("rename rejection target baseline failed", error).at(id)
                })? {
                    FixtureSupport::Supported(false) => {}
                    FixtureSupport::Supported(true) => {
                        return Err(
                            ContractFailure::message_only("rename rejection target baseline was not empty").at(id),
                        );
                    }
                    FixtureSupport::Unsupported => unverifiable = true,
                }
            }
            let failure = match self.fixture.file_system().rename(&source, &target, options) {
                Err(failure) => failure,
                Ok(_) => {
                    return Err(ContractFailure::message_only("unavailable rename requirement succeeded").at(id));
                }
            };
            let error = failure.error();
            if !is_expected_unavailable_rename(
                error,
                failure.state(),
                &source,
                &target,
                self.context.properties().info().provider_id(),
                capability,
            ) {
                return Err(ContractFailure::with_source("rename preflight rejection differs", failure).at(id));
            }
            if let Some(expected) = &baseline {
                match self.fixture.read_file(&source).map_err(|error| {
                    ContractFailure::with_source("rename rejection source final observation failed", error).at(id)
                })? {
                    FixtureSupport::Supported(actual) if actual == *expected => {}
                    FixtureSupport::Supported(_) => {
                        return Err(ContractFailure::message_only("rename rejection changed source contents").at(id));
                    }
                    FixtureSupport::Unsupported => unverifiable = true,
                }
            }
            for (path, expected) in [(&source, source_expected_absent), (&target, true)] {
                match self.fixture.exists_out_of_band(path).map_err(|error| {
                    ContractFailure::with_source("rename rejection final observation failed", error).at(id)
                })? {
                    FixtureSupport::Supported(actual) if actual != expected => {}
                    FixtureSupport::Supported(_) => {
                        return Err(
                            ContractFailure::message_only("rename rejection changed resource existence").at(id),
                        );
                    }
                    FixtureSupport::Unsupported => unverifiable = true,
                }
            }
            self.context.record_check(
                id,
                Some(capability),
                if unverifiable {
                    ContractCheckOutcome::Unverified {
                        reason: "rename rejection lacks complete independent resource observations".to_owned(),
                    }
                } else {
                    ContractCheckOutcome::RejectedAsExpected
                },
            );
            return Ok(());
        }
        let bytes = b"rename source";
        let prepared = self
            .fixture
            .seed_file(&source_relative, bytes)
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
        let initial = self
            .fixture
            .read_file(&source)
            .map_err(|error| ContractFailure::with_source("rename initial source observation failed", error).at(id))?;
        match initial {
            FixtureSupport::Supported(actual) => {
                verify_condition(actual == bytes, id, "rename source fixture contents differ")?;
            }
            FixtureSupport::Unsupported => {
                return self.record_rename_unverified(id, capability, "read_file", "initial source contents");
            }
        }
        if id == ContractCheckId::RenameConflict {
            if let Some((hook, phase)) = self.check_rename_conflict(id, &source, &target, bytes, &options)? {
                return self.record_rename_unverified(id, capability, hook, phase);
            }
            options = options.with_overwrite(true);
        } else {
            let before = self.fixture.exists_out_of_band(&target).map_err(|error| {
                ContractFailure::with_source("rename initial target observation failed", error).at(id)
            })?;
            match before {
                FixtureSupport::Supported(actual) => {
                    verify_condition(!actual, id, "rename target must initially be absent")?;
                }
                FixtureSupport::Unsupported => {
                    return self.record_rename_unverified(
                        id,
                        capability,
                        "exists_out_of_band",
                        "initial target absence",
                    );
                }
            }
        }
        let outcome = self
            .fixture
            .file_system()
            .rename(&source, &target, options)
            .map_err(|error| ContractFailure::with_source("rename/basic: rename failed", error).at(id))?;
        if let Some((hook, phase)) = self.verify_rename_publication(id, &source, &target, bytes, &outcome)? {
            return self.record_rename_unverified(id, capability, hook, phase);
        }
        self.context
            .record_check(id, Some(capability), ContractCheckOutcome::Passed);
        Ok(())
    }

    /// Verifies that the default conflict preserves both independently seeded
    /// files.
    ///
    /// # Parameters
    ///
    /// - `id`: Registered rename-conflict check receiving failures.
    /// - `source`: Independently seeded source that must remain unchanged.
    /// - `target`: Independently seeded conflicting target.
    /// - `bytes`: Expected original source contents.
    /// - `options`: Rename policy whose default conflict behavior is checked.
    ///
    /// # Returns
    ///
    /// `Ok(None)` when the conflict is rejected and both files retain their
    /// original contents, or `Ok(Some((hook, phase)))` when an observation is
    /// unsupported.
    ///
    /// # Errors
    ///
    /// Returns a contract failure when the conflict changes either resource or
    /// its rejection metadata differs.
    fn check_rename_conflict(
        &self,
        id: ContractCheckId,
        source: &Path,
        target: &Path,
        bytes: &[u8],
        options: &RenameOptions,
    ) -> Result<Option<(&'static str, &'static str)>, ContractFailure> {
        let initial_target = self
            .fixture
            .read_file(target)
            .map_err(|error| ContractFailure::with_source("rename initial target observation failed", error).at(id))?;
        match initial_target {
            FixtureSupport::Supported(actual) => {
                verify_condition(
                    actual == b"old target",
                    id,
                    "rename conflict target fixture contents differ",
                )?;
            }
            FixtureSupport::Unsupported => {
                return Ok(Some(("read_file", "initial conflict target contents")));
            }
        }
        let failure = match self.fixture.file_system().rename(source, target, options.clone()) {
            Err(failure) => failure,
            Ok(_) => {
                return Err(ContractFailure::message_only("rename contract: default conflict replaced target").at(id));
            }
        };
        let error = failure.error();
        if failure.state() != RenameFailureState::Unchanged
            || error.kind() != FsErrorKind::AlreadyExists
            || error.operation() != FsOperation::Rename
            || error.path() != Some(source)
            || error.target() != Some(target)
            || error.provider() != Some(self.context.properties().info().provider_id())
        {
            return Err(ContractFailure::with_source("rename conflict rejection differs", failure).at(id));
        }
        for (path, expected, phase) in [
            (source, bytes, "source contents after conflict rejection"),
            (
                target,
                b"old target".as_slice(),
                "target contents after conflict rejection",
            ),
        ] {
            let observed = self
                .fixture
                .read_file(path)
                .map_err(|error| ContractFailure::with_source("rename conflict observation failed", error).at(id))?;
            match observed {
                FixtureSupport::Supported(actual) => {
                    verify_condition(actual == expected, id, "rename conflict changed source or target")?;
                }
                FixtureSupport::Unsupported => return Ok(Some(("read_file", phase))),
            }
        }
        Ok(None)
    }

    /// Checks the reported publication guarantees and observes the final paths.
    ///
    /// # Parameters
    ///
    /// - `id`: Registered rename check selecting any required guarantee.
    /// - `source`: Source path expected in the operation outcome.
    /// - `target`: Target path expected in the operation outcome.
    /// - `bytes`: Independently seeded contents expected at the target.
    /// - `outcome`: Provider-reported rename result to verify.
    ///
    /// # Returns
    ///
    /// `Ok(None)` when the reported paths, guarantees, and observed resource
    /// states satisfy the selected check, or `Ok(Some((hook, phase)))` when an
    /// observation is unsupported.
    ///
    /// # Errors
    ///
    /// Returns a contract failure when reported guarantees or independent
    /// observations differ from the requested rename.
    fn verify_rename_publication(
        &self,
        id: ContractCheckId,
        source: &Path,
        target: &Path,
        bytes: &[u8],
        outcome: &RenameOutcome,
    ) -> Result<Option<(&'static str, &'static str)>, ContractFailure> {
        verify_condition(
            outcome.source() == source && outcome.target() == target,
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
        let exists = self
            .fixture
            .exists_out_of_band(source)
            .map_err(|error| ContractFailure::with_source("rename source removal observation failed", error).at(id))?;
        match exists {
            FixtureSupport::Supported(actual) => {
                verify_condition(!actual, id, "rename/basic: source remained after success")?;
            }
            FixtureSupport::Unsupported => {
                return Ok(Some(("exists_out_of_band", "source removal after rename")));
            }
        }
        let observed = self
            .fixture
            .read_file(target)
            .map_err(|error| ContractFailure::with_source("rename target content observation failed", error).at(id))?;
        match observed {
            FixtureSupport::Supported(actual) => {
                verify_condition(actual == bytes, id, "rename/basic: target bytes mismatch")?;
            }
            FixtureSupport::Unsupported => {
                return Ok(Some(("read_file", "target contents after rename")));
            }
        }
        Ok(None)
    }

    /// Records missing independent observation evidence without failing the
    /// check.
    ///
    /// # Parameters
    ///
    /// * `id` - Rename check receiving the unverified outcome.
    /// * `capability` - Rename capability required by the check.
    /// * `hook` - Fixture observation hook that is unsupported.
    /// * `phase` - Rename observation phase that lacks evidence.
    fn record_rename_unverified(
        &mut self,
        id: ContractCheckId,
        capability: FileSystemCapability,
        hook: &str,
        phase: &str,
    ) -> Result<(), ContractFailure> {
        self.context.record_check(
            id,
            Some(capability),
            ContractCheckOutcome::Unverified {
                reason: format!("rename {phase} lacks {hook} observation support"),
            },
        );
        Ok(())
    }
}
