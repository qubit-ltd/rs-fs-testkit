// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Independently prepared atomic replacement and durable publication checks.

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::AchievedAtomicity;
use qubit_fs::metadata::AtomicityRequirement;
use qubit_fs::metadata::DurabilityRequirement;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteOptions;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FileSystemContractSuite;
use crate::FixturePreparation;
use crate::FixtureSupport;
use crate::WriteScenario;
use crate::internal::verify_condition;
use crate::internal::verify_fs_error;

impl FileSystemContractSuite<'_> {
    /// Executes one strong guarantee check and retains its exact identity.
    pub(super) fn check_write_guarantee(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        let (scenario, capability, name, options) = match id {
            ContractCheckId::WriteAtomicReplaceExisting => (
                WriteScenario::AtomicReplace,
                FileSystemCapability::AtomicReplace,
                "atomic-replace-unavailable",
                WriteOptions::default().with_atomicity(AtomicityRequirement::Required),
            ),
            ContractCheckId::WriteDurable => (
                WriteScenario::Durable,
                FileSystemCapability::DurableWrite,
                "durable-write-unavailable",
                WriteOptions::default()
                    .with_disposition(WriteDisposition::CreateNew)
                    .with_durability(DurabilityRequirement::Required),
            ),
            _ => return Err(ContractFailure::message_only("selected check is not a write guarantee").at(id)),
        };
        let outcome = if !self.capable(FileSystemCapability::Write) {
            ContractCheckOutcome::NotApplicable {
                reason: "Write capability is unavailable".to_owned(),
            }
        } else if !self.capable(capability) {
            let relative = self.context.relative_name(name);
            let path = self
                .fixture
                .path(&relative)
                .map_err(|error| ContractFailure::with_source("write guarantee request path failed", error).at(id))?;
            let error = match self.fixture.file_system().open_writer(&path, options) {
                Err(error) => error,
                Ok(_) => return Err(ContractFailure::message_only("unavailable write guarantee succeeded").at(id)),
            };
            verify_fs_error(
                error,
                FsErrorKind::RequirementNotMet,
                FsOperation::OpenWriter,
                &path,
                self.context.properties().info().provider_id(),
                Some(capability),
                id,
            )?;
            ContractCheckOutcome::RejectedAsExpected
        } else {
            self.execute_write_guarantee(id, scenario)
                .map_err(|failure| failure.at(id))?
        };
        self.context.record_check(id, Some(capability), outcome);
        Ok(())
    }

    /// Verifies the advertised outcome against independently observed contents.
    fn execute_write_guarantee(
        &mut self,
        id: ContractCheckId,
        scenario: WriteScenario,
    ) -> Result<ContractCheckOutcome, ContractFailure> {
        let relative = self.context.relative_name(&id.as_str().replace('/', "-"));
        let bytes = b"b";
        let prepared = self
            .fixture
            .prepare_write(scenario, &relative, bytes)
            .map_err(|error| ContractFailure::with_source("write guarantee preparation failed", error))?;
        let case = match prepared {
            FixturePreparation::Ready(case) => case,
            FixturePreparation::Unavailable { reason } | FixturePreparation::NotApplicable { reason } => {
                return Ok(ContractCheckOutcome::Unverified { reason });
            }
        };
        let path = case.path().clone();
        self.context.record_created(path.clone());
        verify_condition(case.bytes() == bytes, id, "write guarantee fixture changed payload")?;
        if scenario == WriteScenario::AtomicReplace {
            verify_condition(
                case.options().atomicity() == AtomicityRequirement::Required
                    && case.options().disposition() == WriteDisposition::CreateOrReplace,
                id,
                "atomic replacement fixture weakened request semantics",
            )?;
            let before = self.fixture.read_file(&path).map_err(|error| {
                ContractFailure::with_source("atomic replacement initial observation failed", error)
            })?;
            verify_condition(
                matches!(before, FixtureSupport::Supported(old) if old != bytes),
                id,
                "atomic replacement requires an existing target with different bytes",
            )?;
        } else {
            verify_condition(
                case.options().durability() == DurabilityRequirement::Required
                    && case.options().disposition() == WriteDisposition::CreateNew,
                id,
                "durable fixture weakened creation semantics",
            )?;
        }
        let outcome = self
            .fixture
            .file_system()
            .write_all(&path, bytes, case.options().clone())
            .map_err(|error| ContractFailure::with_owned_source("write guarantee publication failed", error))?;
        verify_condition(
            if scenario == WriteScenario::AtomicReplace {
                outcome.atomicity() == AchievedAtomicity::Atomic
            } else {
                outcome.durable()
            },
            id,
            "write guarantee outcome does not satisfy the requirement",
        )?;
        verify_condition(
            outcome.bytes_written().is_none_or(|count| count == bytes.len() as u64),
            id,
            "write guarantee published byte count differs",
        )?;
        let observed = self
            .fixture
            .read_file(&path)
            .map_err(|error| ContractFailure::with_source("write guarantee publication observation failed", error))?;
        verify_condition(
            matches!(observed, FixtureSupport::Supported(actual) if actual == bytes),
            id,
            &format!("{id}: published bytes differ or independent evidence is unavailable"),
        )?;
        Ok(ContractCheckOutcome::Passed)
    }
}
