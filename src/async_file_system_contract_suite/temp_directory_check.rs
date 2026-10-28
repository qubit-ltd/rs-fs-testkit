// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Independent temporary directory publication and lifecycle evidence.

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::error::OpenFailureStage;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::path::Path;
use qubit_fs::path::PathSemantics;
use qubit_fs::temp::TempOptions;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractFailure;
use crate::ContractTempFailure;
use crate::FixtureSupport;
use crate::internal::verify_condition;

mod lifecycle;
mod persistence;

impl AsyncFileSystemContractSuite<'_> {
    /// Executes only the selected temporary lifecycle scenario for this kind.
    pub(super) async fn execute_temp_directory(
        &mut self,
        id: ContractCheckId,
    ) -> Result<Option<String>, ContractFailure> {
        let filesystem = self.fixture.file_system();
        let mut options = TempOptions::default();
        let basic = id == ContractCheckId::TempDirectory;
        if basic && self.capable(FileSystemCapability::TempDirectory) {
            let incompatible = match self.context.properties().info().path_semantics() {
                PathSemantics::Hierarchical => Path::parse_literal("/temp-invalid-parent"),
                _ => Path::parse("/temp-invalid-parent"),
            }
            .expect("invalid-parent probe path is a valid constant");
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
                return Ok(None);
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
        let (temporary, reason) = lifecycle::prepare_temp_directory_persistence(self, id, temporary).await?;
        if reason.is_some() {
            return Ok(reason);
        }
        let Some(temporary) = temporary else {
            return Ok(None);
        };
        if let Some(reason) =
            persistence::persist_temp_directory(self, id, id == ContractCheckId::TempAtomic, temporary).await?
        {
            return Ok(Some(reason));
        }
        if basic {
            return persistence::verify_temp_directory_overwrite(self, id).await;
        }
        Ok(None)
    }
}
