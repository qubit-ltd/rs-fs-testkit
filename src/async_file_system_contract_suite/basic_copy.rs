// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Independent basic copy and operation execution evidence.

use qubit_fs as qfs;
use qubit_fs::copy::CopyFailureState;
use qubit_fs::copy::CopyMethod;
use qubit_fs::copy::CopyOptions;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl AsyncFileSystemContractSuite<'_> {
    /// Executes one basic copy requirement with independently prepared
    /// contents.
    pub(super) async fn check_basic_copy(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        self.context.begin(id.as_str());
        let source_relative = self.context.relative_name("copy-source");
        let target_relative = self.context.relative_name("copy-target");
        let target = self
            .fixture
            .path(&target_relative)
            .map_err(|error| ContractFailure::with_source("copy target preparation failed", error).at(id))?;
        self.context.record_created(target.clone());
        let missing = [
            FileSystemCapability::Read,
            FileSystemCapability::Write,
            FileSystemCapability::Copy,
        ]
        .into_iter()
        .find(|capability| !self.capable(*capability));
        if let Some(required) = missing {
            if id != ContractCheckId::CopyBasic {
                self.context.record_check(
                    id,
                    Some(FileSystemCapability::Copy),
                    ContractCheckOutcome::NotApplicable {
                        reason: "copy prerequisites are unavailable".to_owned(),
                    },
                );
                return Ok(());
            }
            let source = self
                .fixture
                .path(&source_relative)
                .map_err(|error| ContractFailure::with_source("copy source path preparation failed", error).at(id))?;
            let (failure, retained_operation) =
                match self
                    .fixture
                    .file_system()
                    .begin_copy(source.clone(), target.clone(), CopyOptions::file())
                {
                    Err(error) => (error, None),
                    Ok(mut operation) => match operation.execute().await {
                        Err(error) => (error, Some(operation)),
                        Ok(_) => return Err(ContractFailure::message_only("unsupported copy succeeded").at(id)),
                    },
                };
            let path = if required == FileSystemCapability::Write {
                &target
            } else {
                &source
            };
            let error = failure.error();
            if error.kind() != FsErrorKind::UnsupportedCapability
                || error.operation() != FsOperation::Copy
                || error.path() != Some(path)
                || error.target() != Some(&target)
                || error.required_capability() != Some(required)
                || error.provider() != Some(self.context.properties().info().provider_id())
                || failure.state() != CopyFailureState::Unchanged
            {
                return Err(ContractFailure::with_owned_source(
                    "copy rejection context differs",
                    crate::ContractAsyncCopyFailure::new(failure, retained_operation),
                )
                .at(id));
            }
            self.context.record_check(
                id,
                Some(FileSystemCapability::Copy),
                ContractCheckOutcome::RejectedAsExpected,
            );
            return Ok(());
        }
        let bytes = b"copy bytes";
        let scenario = crate::internal::check_catalog::specification(id)
            .copy_scenario
            .ok_or_else(|| ContractFailure::message_only("copy preparation missing from catalog").at(id))?;
        let prepared = self
            .fixture
            .prepare_copy(scenario, &source_relative, &target_relative, bytes)
            .await
            .map_err(|error| ContractFailure::with_source("copy source preparation failed", error).at(id))?;
        let case = match prepared {
            crate::FixturePreparation::Ready(case) => case,
            crate::FixturePreparation::Unavailable { reason } | crate::FixturePreparation::NotApplicable { reason } => {
                self.context.record_check(
                    id,
                    Some(FileSystemCapability::Copy),
                    ContractCheckOutcome::Unverified { reason },
                );
                return Ok(());
            }
        };
        let (source, target, options) = case.into_parts();
        self.context.record_created(target.clone());
        verify_condition(
            options == CopyOptions::file(),
            id,
            "basic copy preparation changed required options",
        )?;
        self.context.record_created(source.clone());
        verify_condition(source != target, id, "copy source and target alias")?;
        let before = self
            .fixture
            .read_file(&source)
            .await
            .map_err(|error| ContractFailure::with_source("copy source observation failed", error).at(id))?;
        verify_condition(
            matches!(before, FixtureSupport::Supported(actual) if actual == bytes),
            id,
            "copy source evidence is unavailable or differs",
        )?;
        let mut operation = self
            .fixture
            .file_system()
            .begin_copy(source.clone(), target.clone(), CopyOptions::file())
            .map_err(|error| {
                ContractFailure::with_owned_source(
                    "copy admission failed",
                    crate::ContractAsyncCopyFailure::new(error, None),
                )
                .at(id)
            })?;
        drop(operation.execute());
        verify_condition(
            operation.state() == qfs::copy::AsyncCopyOperationState::Ready && !operation.has_recovery(),
            id,
            "unpolled copy changed operation state",
        )?;
        let outcome = match operation.execute().await {
            Ok(outcome) => outcome,
            Err(error) => {
                return Err(ContractFailure::with_owned_source(
                    "copy execution failed",
                    crate::ContractAsyncCopyFailure::new(error, Some(operation)),
                )
                .at(id));
            }
        };
        verify_condition(
            outcome.stats().bytes == bytes.len() as u64 && outcome.stats().files + outcome.stats().objects == 1,
            id,
            "copy statistics differ",
        )?;
        verify_condition(
            outcome.used_fallback() == (outcome.method() == CopyMethod::Streamed),
            id,
            "copy method and fallback evidence disagree",
        )?;
        verify_condition(
            !self.fixture.copy_fallback_only() || outcome.method() == CopyMethod::Streamed,
            id,
            "fallback-only copy reported a native method",
        )?;
        verify_condition(
            operation.state() == qfs::copy::AsyncCopyOperationState::Completed,
            id,
            "successful copy did not complete the operation",
        )?;
        if id == ContractCheckId::CopyRepeatedExecute {
            for path in [&source, &target] {
                let observed =
                    self.fixture.read_file(path).await.map_err(|error| {
                        ContractFailure::with_source("initial copy observation failed", error).at(id)
                    })?;
                verify_condition(
                    matches!(observed, FixtureSupport::Supported(actual) if actual == bytes),
                    id,
                    "initial copy contents differ or evidence is unavailable",
                )?;
            }

            let repeated = match operation.execute().await {
                Err(error) => error,
                Ok(_) => return Err(ContractFailure::message_only("copy executed twice").at(id)),
            };
            if repeated.error().kind() != FsErrorKind::InvalidState
                || repeated.state() != CopyFailureState::Published
                || repeated.partial_stats() != outcome.stats()
            {
                return Err(ContractFailure::with_owned_source(
                    "repeated copy lost completed facts",
                    crate::ContractAsyncCopyFailure::new(repeated, Some(operation)),
                )
                .at(id));
            }
            verify_condition(
                operation.state() == qfs::copy::AsyncCopyOperationState::Completed,
                id,
                "repeat changed completed copy state",
            )?;
        }
        for path in [&source, &target] {
            let observed = self
                .fixture
                .read_file(path)
                .await
                .map_err(|error| ContractFailure::with_source("copy contents observation failed", error).at(id))?;
            verify_condition(
                matches!(observed, FixtureSupport::Supported(actual) if actual == bytes),
                id,
                "copy source or target contents differ or evidence is unavailable",
            )?;
        }
        self.context
            .record_check(id, Some(FileSystemCapability::Copy), ContractCheckOutcome::Passed);
        Ok(())
    }
}
