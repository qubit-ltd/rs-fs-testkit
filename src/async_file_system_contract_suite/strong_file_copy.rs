// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Independently prepared file copy publication guarantees.

use qubit_fs::copy::CopyFailureState;
use qubit_fs::copy::CopyOptions;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::AchievedAtomicity;
use qubit_fs::metadata::AtomicityRequirement;
use qubit_fs::metadata::DurabilityRequirement;
use qubit_fs::metadata::FileSystemCapability;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::CopyScenario;
use crate::FixturePreparation;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl AsyncFileSystemContractSuite<'_> {
    /// Verifies exactly one required guarantee against independently observed
    /// bytes.
    pub(super) async fn check_strong_file_copy(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        self.context.begin(id.as_str());
        let spec = crate::internal::check_catalog::specification(id);
        let scenario = spec
            .copy_scenario
            .ok_or_else(|| ContractFailure::message_only("strong copy scenario absent from catalog").at(id))?;
        let (capability, options) = match scenario {
            CopyScenario::AtomicFile => (
                FileSystemCapability::AtomicFileCopy,
                CopyOptions::file().with_atomicity(AtomicityRequirement::Required),
            ),
            CopyScenario::DurableFile => (
                FileSystemCapability::DurableFileCopy,
                CopyOptions::file().with_durability(DurabilityRequirement::Required),
            ),
            _ => return Err(ContractFailure::message_only("selected check is not a file-copy guarantee").at(id)),
        };
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
        let source_relative = self.context.relative_name("strong-copy-source");
        let target_relative = self.context.relative_name("strong-copy-target");
        if !self.capable(capability) {
            let source = self
                .fixture
                .path(&source_relative)
                .map_err(|error| ContractFailure::with_source("strong copy source path failed", error).at(id))?;
            let target = self
                .fixture
                .path(&target_relative)
                .map_err(|error| ContractFailure::with_source("strong copy target path failed", error).at(id))?;
            self.context.record_created(target.clone());
            let failure = match crate::internal::execute_copy::execute_copy(
                self.fixture.file_system(),
                source.clone(),
                target.clone(),
                options,
            )
            .await
            {
                Err(failure) => failure,
                Ok(_) => return Err(ContractFailure::message_only("unavailable copy guarantee succeeded").at(id)),
            };
            let error = failure.error();
            if error.kind() != FsErrorKind::RequirementNotMet
                || error.operation() != FsOperation::Copy
                || error.required_capability() != Some(capability)
                || error.path() != Some(&source)
                || error.target() != Some(&target)
                || error.provider() != Some(self.context.properties().info().provider_id())
                || failure.failure().state() != CopyFailureState::Unchanged
            {
                return Err(ContractFailure::with_owned_source("strong copy rejection differs", failure).at(id));
            }
            self.context
                .record_check(id, Some(capability), ContractCheckOutcome::RejectedAsExpected);
            return Ok(());
        }
        let bytes = b"strong copy contents";
        let prepared = self
            .fixture
            .prepare_copy(scenario, &source_relative, &target_relative, bytes)
            .await
            .map_err(|error| ContractFailure::with_source("strong copy preparation failed", error).at(id))?;
        let case = match prepared {
            FixturePreparation::Ready(case) => case,
            FixturePreparation::Unavailable { reason } | FixturePreparation::NotApplicable { reason } => {
                self.context
                    .record_check(id, Some(capability), ContractCheckOutcome::Unverified { reason });
                return Ok(());
            }
        };
        let (source, target, prepared_options) = case.into_parts();
        self.context.record_created(source.clone());
        self.context.record_created(target.clone());
        verify_condition(
            source != target && prepared_options == options,
            id,
            "strong copy preparation changed required request semantics",
        )?;
        let source_before = self
            .fixture
            .read_file(&source)
            .await
            .map_err(|error| ContractFailure::with_source("strong copy source observation failed", error).at(id))?;
        let target_before = self.fixture.exists_out_of_band(&target).await.map_err(|error| {
            ContractFailure::with_source("strong copy destination observation failed", error).at(id)
        })?;
        let (FixtureSupport::Supported(actual), FixtureSupport::Supported(exists)) = (source_before, target_before)
        else {
            self.context.record_check(
                id,
                Some(capability),
                ContractCheckOutcome::Unverified {
                    reason: "strong copy requires independent source bytes and target absence".to_owned(),
                },
            );
            return Ok(());
        };
        verify_condition(actual == bytes && !exists, id, "strong copy initial state differs")?;
        let outcome = crate::internal::execute_copy::execute_copy(
            self.fixture.file_system(),
            source.clone(),
            target.clone(),
            options,
        )
        .await
        .map_err(|error| ContractFailure::with_owned_source("strong copy execution failed", error).at(id))?;
        let achieved = match scenario {
            CopyScenario::AtomicFile => outcome.atomicity() == AchievedAtomicity::Atomic,
            CopyScenario::DurableFile => outcome.durable(),
            _ => false,
        };
        verify_condition(achieved, id, "copy did not achieve its required publication guarantee")?;
        verify_condition(
            outcome.stats().bytes == bytes.len() as u64 && outcome.stats().files + outcome.stats().objects == 1,
            id,
            "strong copy statistics differ",
        )?;
        for path in [&source, &target] {
            let observed =
                self.fixture.read_file(path).await.map_err(|error| {
                    ContractFailure::with_source("strong copy final observation failed", error).at(id)
                })?;
            verify_condition(
                matches!(observed, FixtureSupport::Supported(actual) if actual == bytes),
                id,
                "strong copy changed source or published incorrect target contents",
            )?;
        }
        self.context
            .record_check(id, Some(capability), ContractCheckOutcome::Passed);
        Ok(())
    }
}
