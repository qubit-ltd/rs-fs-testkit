// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Independent temporary directory publication and lifecycle evidence.

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::error::OpenFailureStage;
use qubit_fs::metadata::AchievedAtomicity;
use qubit_fs::metadata::AtomicityRequirement;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::path::Path;
use qubit_fs::path::PathSemantics;
use qubit_fs::temp::PersistFailureState;
use qubit_fs::temp::PersistOptions;
use qubit_fs::temp::TempOptions;
use qubit_fs::temp::TempResourceState;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractFailure;
use crate::ContractTempFailure;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl AsyncFileSystemContractSuite<'_> {
    /// Executes only the selected temporary lifecycle scenario for this kind.
    pub(super) async fn execute_temp_directory(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        let filesystem = self.fixture.file_system();
        let mut options = TempOptions::default();
        let basic = id == ContractCheckId::TempDirectory;
        if basic && self.capable(FileSystemCapability::TempDirectory) {
            let incompatible = match self.context.properties().info().path_semantics() {
                PathSemantics::Hierarchical => Path::parse_literal("/temp-invalid-parent"),
                _ => Path::parse("/temp-invalid-parent"),
            }
            .map_err(|error| {
                ContractFailure::with_source("temporary invalid-parent request preparation failed", error).at(id)
            })?;
            let error = match filesystem
                .create_temp_directory(TempOptions::default().with_parent(Some(incompatible.clone())))
                .await
            {
                Err(error) => error,
                Ok(resource) => {
                    return Err(ContractFailure::with_owned_source(
                        "invalid temporary parent succeeded",
                        ContractTempFailure::new(
                            ContractFailure::message_only("invalid temporary parent was accepted"),
                            resource,
                        ),
                    )
                    .at(id));
                }
            };
            if error.recovery().is_some()
                || error.stage() != OpenFailureStage::Preflight
                || error.error().kind() != FsErrorKind::InvalidPath
                || error.error().operation() != FsOperation::CreateTemp
                || error.error().path() != Some(&incompatible)
            {
                return Err(ContractFailure::with_owned_source("temporary parent validation differs", error).at(id));
            }
            if self.capable(FileSystemCapability::CreateDirectory) {
                let relative = self.context.relative_name("temp-directory-parent");
                let parent = self.fixture.seed_empty_directory(&relative).await.map_err(|error| {
                    ContractFailure::with_source("temporary parent preparation failed", error).at(id)
                })?;
                let FixtureSupport::Supported(parent) = parent else {
                    return Err(ContractFailure::message_only(
                        "temporary parent needs independent fixture preparation",
                    )
                    .at(id));
                };
                self.context.record_created(parent.clone());
                options = options.with_parent(Some(parent));
            }
            options = options
                .with_prefix("contract-directory-".to_owned())
                .with_suffix(".tmp".to_owned());
        }
        let requested_parent = options.parent().cloned();
        let mut temporary = match filesystem.create_temp_directory(options).await {
            Ok(resource) => resource,
            Err(error) if !self.capable(FileSystemCapability::TempDirectory) => {
                if error.recovery().is_some()
                    || error.stage() != OpenFailureStage::Preflight
                    || error.error().kind() != FsErrorKind::UnsupportedCapability
                    || error.error().operation() != FsOperation::CreateTemp
                    || error.error().required_capability() != Some(FileSystemCapability::TempDirectory)
                {
                    return Err(
                        ContractFailure::with_owned_source("temporary creation rejection differs", error).at(id),
                    );
                }
                return Ok(());
            }
            Err(error) => {
                return Err(ContractFailure::with_owned_source("temporary resource creation failed", error).at(id));
            }
        };
        let source = temporary.path().clone();
        self.context.record_created(source.clone());
        verify_condition(
            self.capable(FileSystemCapability::TempDirectory),
            id,
            "unavailable temporary creation succeeded",
        )?;
        if basic {
            if let Some(parent) = requested_parent {
                verify_condition(
                    source
                        .as_str()
                        .starts_with(&format!("{}/", parent.as_str().trim_end_matches('/'))),
                    id,
                    "temporary resource ignored requested parent",
                )?;
            }
            verify_condition(
                source.as_str().contains("contract-directory-") && source.as_str().ends_with(".tmp"),
                id,
                "temp/directory: requested prefix or suffix was ignored",
            )?;
            if let Err(error) = temporary.cleanup().await {
                return Err(ContractFailure::with_owned_source(
                    "temporary cleanup failed",
                    ContractTempFailure::new(error, temporary),
                )
                .at(id));
            }
            let after =
                self.fixture.exists_out_of_band(&source).await.map_err(|error| {
                    ContractFailure::with_source("temporary cleanup observation failed", error).at(id)
                })?;
            verify_condition(
                matches!(after, FixtureSupport::Supported(false)),
                id,
                "temp/directory: cleanup retained source",
            )?;
            temporary = filesystem
                .create_temp_directory(TempOptions::default())
                .await
                .map_err(|error| {
                    ContractFailure::with_owned_source("temporary keep preparation failed", error).at(id)
                })?;
            self.context.record_created(temporary.path().clone());
        }
        if id != ContractCheckId::TempAtomic {
            let kept_source = temporary.path().clone();
            let kept = match temporary.keep().await {
                Ok(outcome) => outcome,
                Err(error) => {
                    return Err(ContractFailure::with_owned_source(
                        "temporary keep failed",
                        ContractTempFailure::new(error, temporary),
                    )
                    .at(id));
                }
            };
            self.context.record_created(kept.target().clone());
            verify_condition(
                temporary.state() == TempResourceState::Kept && kept.target() != &kept_source,
                id,
                "temporary keep state or publication identity differs",
            )?;
            for (path, expected) in [(&kept_source, false), (kept.target(), true)] {
                let observed =
                    self.fixture.exists_out_of_band(path).await.map_err(|error| {
                        ContractFailure::with_source("temporary keep observation failed", error).at(id)
                    })?;
                verify_condition(
                    matches!(observed, FixtureSupport::Supported(actual) if actual == expected),
                    id,
                    "temporary keep publication differs",
                )?;
            }
            if id == ContractCheckId::TempRepeatedLifecycle {
                let failure = match temporary.keep().await {
                    Err(failure) => failure,
                    Ok(_) => {
                        return Err(
                            ContractFailure::message_only("temp/repeated-lifecycle: second keep succeeded").at(id),
                        );
                    }
                };
                if failure.error().kind() != FsErrorKind::InvalidState
                    || failure.state() != PersistFailureState::PublishedSourceReleased
                    || failure.publication_target() != Some(kept.target())
                {
                    return Err(ContractFailure::with_owned_source(
                        "temp/repeated-lifecycle: publication facts lost",
                        ContractTempFailure::new(failure, temporary),
                    )
                    .at(id));
                }
                let error = match temporary.cleanup().await {
                    Err(error) => error,
                    Ok(_) => {
                        return Err(
                            ContractFailure::message_only("temp/repeated-lifecycle: kept target reclaimed").at(id),
                        );
                    }
                };
                if error.kind() != FsErrorKind::InvalidState {
                    return Err(ContractFailure::with_owned_source(
                        "repeated cleanup rejection differs",
                        ContractTempFailure::new(error, temporary),
                    )
                    .at(id));
                }
                verify_condition(temporary.state() == TempResourceState::Kept, id, "kept state changed")?;
                let after = self.fixture.exists_out_of_band(kept.target()).await.map_err(|error| {
                    ContractFailure::with_source("repeated keep publication observation failed", error).at(id)
                })?;
                verify_condition(
                    matches!(after, FixtureSupport::Supported(true)),
                    id,
                    "kept publication disappeared",
                )?;
                let mut cleaned = filesystem
                    .create_temp_directory(TempOptions::default())
                    .await
                    .map_err(|error| {
                        ContractFailure::with_owned_source("repeated cleanup preparation failed", error).at(id)
                    })?;
                let cleaned_source = cleaned.path().clone();
                self.context.record_created(cleaned_source.clone());
                if let Err(error) = cleaned.cleanup().await {
                    return Err(ContractFailure::with_owned_source(
                        "initial cleanup failed",
                        ContractTempFailure::new(error, cleaned),
                    )
                    .at(id));
                }
                let after = self
                    .fixture
                    .exists_out_of_band(&cleaned_source)
                    .await
                    .map_err(|error| {
                        ContractFailure::with_source("repeated cleanup observation failed", error).at(id)
                    })?;
                verify_condition(
                    matches!(after, FixtureSupport::Supported(false)),
                    id,
                    "cleanup retained temporary source",
                )?;
                let error = match cleaned.cleanup().await {
                    Err(error) => error,
                    Ok(_) => {
                        return Err(
                            ContractFailure::message_only("temp/repeated-lifecycle: cleaned resource reused").at(id),
                        );
                    }
                };
                if error.kind() != FsErrorKind::InvalidState || cleaned.state() != TempResourceState::Cleaned {
                    return Err(ContractFailure::with_owned_source(
                        "repeated cleanup state differs",
                        ContractTempFailure::new(error, cleaned),
                    )
                    .at(id));
                }
                return Ok(());
            }
            temporary = filesystem
                .create_temp_directory(TempOptions::default())
                .await
                .map_err(|error| {
                    ContractFailure::with_owned_source("temporary persistence preparation failed", error).at(id)
                })?;
            self.context.record_created(temporary.path().clone());
        }
        let source = temporary.path().clone();
        let target = self
            .fixture
            .path(&self.context.relative_name("persisted-directory"))
            .map_err(|error| {
                ContractFailure::with_source("temporary publication target preparation failed", error).at(id)
            })?;
        self.context.record_created(target.clone());
        let atomic = id == ContractCheckId::TempAtomic;
        let options = PersistOptions::default().with_atomicity(if atomic {
            AtomicityRequirement::Required
        } else {
            AtomicityRequirement::Preferred
        });
        let publication = temporary.persist(&target, options).await;
        if atomic && !self.capable(FileSystemCapability::AtomicTempPersist) {
            let failure = match publication {
                Err(failure) => failure,
                Ok(_) => return Err(ContractFailure::message_only("unsupported atomic persistence succeeded").at(id)),
            };
            let error = failure.error();
            if error.kind() != FsErrorKind::RequirementNotMet
                || error.operation() != FsOperation::PersistTemp
                || error.path() != Some(&source)
                || error.target() != Some(&target)
                || error.required_capability() != Some(FileSystemCapability::AtomicTempPersist)
                || error.provider() != Some(self.context.properties().info().provider_id())
                || failure.state() != PersistFailureState::NotPublished
            {
                return Err(ContractFailure::with_owned_source(
                    "atomic persistence rejection differs",
                    ContractTempFailure::new(failure, temporary),
                )
                .at(id));
            }
            let before = self.fixture.exists_out_of_band(&source).await.map_err(|error| {
                ContractFailure::with_source("atomic rejection source observation failed", error).at(id)
            })?;
            verify_condition(
                matches!(before, FixtureSupport::Supported(true)),
                id,
                "atomic preflight removed source",
            )?;
            verify_condition(
                temporary.state() == TempResourceState::Owned,
                id,
                "atomic preflight changed resource ownership",
            )?;
            let target_after = self.fixture.exists_out_of_band(&target).await.map_err(|error| {
                ContractFailure::with_source("atomic rejection target observation failed", error).at(id)
            })?;
            verify_condition(
                matches!(target_after, FixtureSupport::Supported(false)),
                id,
                "atomic preflight published a target",
            )?;
            if let Err(error) = temporary.cleanup().await {
                return Err(ContractFailure::with_owned_source(
                    "atomic rejection cleanup failed",
                    ContractTempFailure::new(error, temporary),
                )
                .at(id));
            }
            let after = self.fixture.exists_out_of_band(&source).await.map_err(|error| {
                ContractFailure::with_source("atomic rejection cleanup observation failed", error).at(id)
            })?;
            verify_condition(
                matches!(after, FixtureSupport::Supported(false)),
                id,
                "atomic rejection cleanup retained source",
            )?;
            return Ok(());
        }
        let outcome = match publication {
            Ok(outcome) => outcome,
            Err(error) => {
                return Err(ContractFailure::with_owned_source(
                    "temp/atomic: persistence failed",
                    ContractTempFailure::new(error, temporary),
                )
                .at(id));
            }
        };
        verify_condition(outcome.target() == &target, id, "temporary persist target mismatch")?;
        if atomic {
            verify_condition(
                outcome.atomicity() == AchievedAtomicity::Atomic,
                id,
                "temp/atomic: non-atomic publication",
            )?;
        }
        for (path, expected) in [(&source, false), (&target, true)] {
            let observed =
                self.fixture.exists_out_of_band(path).await.map_err(|error| {
                    ContractFailure::with_source("temporary persist observation failed", error).at(id)
                })?;
            verify_condition(
                matches!(observed, FixtureSupport::Supported(actual) if actual == expected),
                id,
                "temporary persistence source release or target publication differs",
            )?;
        }
        if basic {
            let relative = self.context.relative_name("temp-overwritten-directory");
            let prepared = self.fixture.seed_empty_directory(&relative).await.map_err(|error| {
                ContractFailure::with_source("temporary overwrite target preparation failed", error).at(id)
            })?;
            let FixtureSupport::Supported(target) = prepared else {
                return Err(ContractFailure::message_only(
                    "temporary directory overwrite needs independent target preparation",
                )
                .at(id));
            };
            self.context.record_created(target.clone());
            let mut replacement = filesystem
                .create_temp_directory(TempOptions::default())
                .await
                .map_err(|error| {
                    ContractFailure::with_owned_source("temporary overwrite preparation failed", error).at(id)
                })?;
            let source = replacement.path().clone();
            self.context.record_created(source.clone());
            let outcome = match replacement
                .persist(
                    &target,
                    PersistOptions::default()
                        .with_overwrite(true)
                        .with_atomicity(AtomicityRequirement::Preferred),
                )
                .await
            {
                Ok(outcome) => outcome,
                Err(error) => {
                    return Err(ContractFailure::with_owned_source(
                        "temporary directory overwrite failed",
                        ContractTempFailure::new(error, replacement),
                    )
                    .at(id));
                }
            };
            verify_condition(outcome.target() == &target, id, "temporary overwrite target differs")?;
            for (path, expected) in [(&source, false), (&target, true)] {
                let observed = self.fixture.exists_out_of_band(path).await.map_err(|error| {
                    ContractFailure::with_source("temporary overwrite observation failed", error).at(id)
                })?;
                verify_condition(
                    matches!(observed, FixtureSupport::Supported(actual) if actual == expected),
                    id,
                    "temporary overwrite did not release source and publish target",
                )?;
            }
        }
        Ok(())
    }
}
