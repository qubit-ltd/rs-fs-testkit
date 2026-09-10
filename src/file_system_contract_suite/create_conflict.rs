// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Creation conflict evidence independent of successful basic writes.

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::write::WriteDisposition;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FileSystemContractSuite;
use crate::FixturePreparation;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl FileSystemContractSuite<'_> {
    /// Executes one independently prepared conflict and records its evidence.
    pub(super) fn check_create_conflict(&mut self) -> Result<(), ContractFailure> {
        let id = ContractCheckId::WriteCreateConflict;
        let outcome = self.execute_create_conflict().map_err(|failure| failure.at(id))?;
        self.context
            .record_check(id, Some(FileSystemCapability::Write), outcome);
        Ok(())
    }

    /// Requires both real rejection and preservation of independently seeded
    /// bytes.
    fn execute_create_conflict(&mut self) -> Result<ContractCheckOutcome, ContractFailure> {
        let id = ContractCheckId::WriteCreateConflict;
        if !self.capable(FileSystemCapability::Write) {
            return Ok(ContractCheckOutcome::NotApplicable {
                reason: "Write capability is unavailable".to_owned(),
            });
        }
        let bytes = super::write::bounded_payload(
            self.context.properties().limits().max_write_bytes(),
            b"unexpected",
            b'u',
        );
        let relative = self.context.relative_name("write-create-conflict");
        let prepared = self
            .fixture
            .prepare_write(
                crate::internal::check_catalog::specification(id)
                    .write_scenario
                    .expect("write check has a preparation scenario"),
                &relative,
                &bytes,
            )
            .map_err(|error| ContractFailure::with_source("creation conflict preparation failed", error))?;
        let case = match prepared {
            FixturePreparation::Ready(case) => case,
            FixturePreparation::Unavailable { reason } | FixturePreparation::NotApplicable { reason } => {
                return Ok(ContractCheckOutcome::Unverified { reason });
            }
        };
        let path = case.path().clone();
        self.context.record_created(path.clone());
        verify_condition(
            case.bytes() == bytes && case.options().disposition() == WriteDisposition::CreateNew,
            id,
            "creation conflict fixture changed request semantics",
        )?;
        let before = self
            .fixture
            .read_file(&path)
            .map_err(|error| ContractFailure::with_source("creation conflict initial observation failed", error))?;
        let FixtureSupport::Supported(before) = before else {
            return Ok(ContractCheckOutcome::Unverified {
                reason: "creation conflict initial content unavailable".to_owned(),
            });
        };
        verify_condition(
            before != bytes,
            id,
            "creation conflict seed must differ from requested bytes",
        )?;
        let failure = match self
            .fixture
            .file_system()
            .write_all(&path, &bytes, case.options().clone())
        {
            Err(failure) => failure,
            Ok(_) => return Err(ContractFailure::message_only("CreateNew accepted an existing target")),
        };
        let error = failure.error();
        if error.kind() != FsErrorKind::AlreadyExists
            || !matches!(error.operation(), FsOperation::OpenWriter | FsOperation::CommitWriter)
            || error.path() != Some(&path)
            || error.provider() != Some(self.context.properties().info().provider_id())
        {
            return Err(ContractFailure::with_owned_source(
                "creation conflict rejection differs",
                failure,
            ));
        }
        crate::internal::finish_expected_write_failure::finish_expected_write_failure(failure, id)?;
        let observed = self
            .fixture
            .read_file(&path)
            .map_err(|error| ContractFailure::with_source("creation conflict final observation failed", error))?;
        verify_condition(
            matches!(observed, FixtureSupport::Supported(actual) if actual == before),
            id,
            "creation conflict changed the target or independent evidence is unavailable",
        )?;
        Ok(ContractCheckOutcome::RejectedAsExpected)
    }
}
