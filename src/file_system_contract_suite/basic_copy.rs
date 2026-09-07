// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Independent basic copy and operation execution evidence.

use qubit_fs::copy::CopyFailureState;
use qubit_fs::copy::CopyMethod;
use qubit_fs::copy::CopyOptions;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FileSystemContractSuite;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl FileSystemContractSuite<'_> {
    /// Executes one basic copy requirement with independently prepared
    /// contents.
    pub(super) fn check_basic_copy(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
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
            let failure = match self.fixture.file_system().copy(&source, &target, CopyOptions::file()) {
                Err(error) => error,
                Ok(_) => return Err(ContractFailure::message_only("unsupported copy succeeded").at(id)),
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
                return Err(ContractFailure::with_owned_source("copy rejection context differs", failure).at(id));
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
            .map_err(|error| ContractFailure::with_source("copy source observation failed", error).at(id))?;
        verify_condition(
            matches!(before, FixtureSupport::Supported(actual) if actual == bytes),
            id,
            "copy source evidence is unavailable or differs",
        )?;
        let outcome = self
            .fixture
            .file_system()
            .copy(&source, &target, CopyOptions::file())
            .map_err(|error| ContractFailure::with_owned_source("copy execution failed", error).at(id))?;
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

        for path in [&source, &target] {
            let observed = self
                .fixture
                .read_file(path)
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
