// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Independently observed server-side copy requirements.

use qubit_fs::copy::CopyConflictPolicy;
use qubit_fs::copy::CopyFailureState;
use qubit_fs::copy::CopyMethod;
use qubit_fs::copy::CopyMode;
use qubit_fs::copy::CopyOptions;
use qubit_fs::copy::ServerSidePreference;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FileSystemContractSuite;
use crate::FixturePreparation;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl FileSystemContractSuite<'_> {
    /// Requires an actual server-side outcome or an actual capability
    /// rejection.
    pub(super) fn check_server_side_copy(&mut self) -> Result<(), ContractFailure> {
        let id = ContractCheckId::CopyServerSide;
        let capability = FileSystemCapability::ServerSideCopy;
        self.context.begin(id.as_str());
        if !self.capable(FileSystemCapability::Copy) {
            self.context.record_check(
                id,
                Some(capability),
                ContractCheckOutcome::NotApplicable {
                    reason: "Copy capability is unavailable".to_owned(),
                },
            );
            return Ok(());
        }
        let source_relative = self.context.relative_name("server-side-source");
        let target_relative = self.context.relative_name("server-side-target");
        if !self.capable(capability) {
            let source = self
                .fixture
                .path(&source_relative)
                .map_err(|error| ContractFailure::with_source("server-side source path failed", error).at(id))?;
            let target = self
                .fixture
                .path(&target_relative)
                .map_err(|error| ContractFailure::with_source("server-side target path failed", error).at(id))?;
            self.context.record_created(target.clone());
            let options = CopyOptions::file().with_server_side(ServerSidePreference::Require);
            let failure = match self.fixture.file_system().copy(&source, &target, options) {
                Err(failure) => failure,
                Ok(_) => {
                    return Err(ContractFailure::message_only("unavailable server-side requirement succeeded").at(id));
                }
            };
            let error = failure.error();
            if error.kind() != FsErrorKind::RequirementNotMet
                || error.operation() != FsOperation::Copy
                || error.required_capability() != Some(capability)
                || error.path() != Some(&source)
                || error.target() != Some(&target)
                || error.provider() != Some(self.context.properties().info().provider_id())
                || failure.state() != CopyFailureState::Unchanged
            {
                return Err(
                    ContractFailure::with_owned_source("server-side rejection context differs", failure).at(id),
                );
            }
            self.context
                .record_check(id, Some(capability), ContractCheckOutcome::RejectedAsExpected);
            return Ok(());
        }
        let scenario = crate::internal::check_catalog::specification(id)
            .copy_scenario
            .ok_or_else(|| ContractFailure::message_only("server-side scenario absent from catalog").at(id))?;
        let prepared = self
            .fixture
            .prepare_copy(scenario, &source_relative, &target_relative, b"server-side")
            .map_err(|error| ContractFailure::with_source("server-side preparation failed", error).at(id))?;
        let case = match prepared {
            FixturePreparation::Ready(case) => case,
            FixturePreparation::Unavailable { reason } | FixturePreparation::NotApplicable { reason } => {
                self.context
                    .record_check(id, Some(capability), ContractCheckOutcome::Unverified { reason });
                return Ok(());
            }
        };
        let (source, target, options) = case.into_parts();
        self.context.record_created(source.clone());
        self.context.record_created(target.clone());
        verify_condition(
            source != target
                && options.mode() == CopyMode::File
                && options.server_side() == ServerSidePreference::Require
                && options.conflict() == CopyConflictPolicy::Fail,
            id,
            "server-side preparation changed required request semantics",
        )?;
        let snapshot = match self.fixture.read_file(&source).map_err(|error| {
            ContractFailure::with_source("server-side initial source observation failed", error).at(id)
        })? {
            FixtureSupport::Supported(bytes) => bytes,
            FixtureSupport::Unsupported => {
                self.context.record_check(
                    id,
                    Some(capability),
                    ContractCheckOutcome::Unverified {
                        reason: "independent server-side source observation unavailable".to_owned(),
                    },
                );
                return Ok(());
            }
        };
        verify_condition(
            snapshot.len() <= 64 * 1024,
            id,
            "server-side fixture source exceeds 64 KiB probe budget",
        )?;
        let before = self.fixture.exists_out_of_band(&target).map_err(|error| {
            ContractFailure::with_source("server-side destination absence observation failed", error).at(id)
        })?;
        let FixtureSupport::Supported(exists) = before else {
            self.context.record_check(
                id,
                Some(capability),
                ContractCheckOutcome::Unverified {
                    reason: "independent server-side destination absence observation unavailable".to_owned(),
                },
            );
            return Ok(());
        };
        verify_condition(!exists, id, "server-side destination already exists")?;
        let outcome = self
            .fixture
            .file_system()
            .copy(&source, &target, options)
            .map_err(|error| ContractFailure::with_owned_source("server-side execution failed", error).at(id))?;
        verify_condition(
            outcome.method() == CopyMethod::ServerSide && !outcome.used_fallback(),
            id,
            "copy did not provide the required server-side method",
        )?;
        verify_condition(
            outcome.stats().bytes == snapshot.len() as u64 && outcome.stats().files + outcome.stats().objects == 1,
            id,
            "server-side copy statistics differ",
        )?;
        for path in [&source, &target] {
            let observed = self.fixture.read_file(path).map_err(|error| {
                ContractFailure::with_source("server-side final contents observation failed", error).at(id)
            })?;
            verify_condition(
                matches!(observed, FixtureSupport::Supported(actual) if actual == snapshot),
                id,
                "server-side copy changed source or published incorrect target bytes",
            )?;
        }
        self.context
            .record_check(id, Some(capability), ContractCheckOutcome::Passed);
        Ok(())
    }
}
