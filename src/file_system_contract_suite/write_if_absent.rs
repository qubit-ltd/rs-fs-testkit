// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Conditional creation with independent request preparation and observations.

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteOptions;
use qubit_fs::write::WritePrecondition;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FileSystemContractSuite;
use crate::FixturePreparation;
use crate::FixtureSupport;
use crate::internal::verify_condition;
use crate::internal::verify_fs_error;

impl FileSystemContractSuite<'_> {
    /// Records only the If-Absent check, retaining original errors on failure.
    pub(super) fn check_write_if_absent(&mut self) -> Result<(), ContractFailure> {
        let outcome = self.execute_write_if_absent()?;
        self.context.record_check(
            ContractCheckId::WriteIfAbsent,
            Some(FileSystemCapability::ConditionalWrite),
            outcome,
        );
        Ok(())
    }

    /// Publishes once, rejects a second publication, and observes retained
    /// bytes.
    fn execute_write_if_absent(&mut self) -> Result<ContractCheckOutcome, ContractFailure> {
        let id = ContractCheckId::WriteIfAbsent;
        let relative = self.context.relative_name("write-conditional");
        if !self.capable(FileSystemCapability::ConditionalWrite) {
            let path = self
                .fixture
                .path(&relative)
                .map_err(|error| ContractFailure::with_source("conditional write path failed", error).at(id))?;
            let options = WriteOptions::default().with_precondition(WritePrecondition::IfAbsent);
            let error = match self.fixture.file_system().open_writer(&path, options) {
                Err(error) => error,
                Ok(_) => return Err(ContractFailure::message_only("unavailable conditional write succeeded").at(id)),
            };
            verify_fs_error(
                error,
                FsErrorKind::RequirementNotMet,
                FsOperation::OpenWriter,
                &path,
                self.context.properties().info().provider_id(),
                Some(FileSystemCapability::ConditionalWrite),
                id,
            )?;
            return Ok(ContractCheckOutcome::RejectedAsExpected);
        }
        let limit = self.context.properties().limits().max_write_bytes();
        let bytes = super::write::bounded_payload(limit, b"conditional", b'c');
        let prepared = self
            .fixture
            .prepare_write(
                crate::internal::check_catalog::specification(id)
                    .write_scenario
                    .expect("write check has a preparation scenario"),
                &relative,
                &bytes,
            )
            .map_err(|error| ContractFailure::with_source("If-Absent preparation failed", error).at(id))?;
        let case = match prepared {
            FixturePreparation::Ready(case) => case,
            FixturePreparation::Unavailable { reason } | FixturePreparation::NotApplicable { reason } => {
                return Ok(ContractCheckOutcome::Unverified { reason });
            }
        };
        verify_condition(
            case.bytes() == bytes
                && case.options().precondition() == &WritePrecondition::IfAbsent
                && case.options().disposition() == WriteDisposition::CreateOrReplace,
            id,
            "fixture changed If-Absent request semantics",
        )?;
        let path = case.path().clone();
        self.context.record_created(path.clone());
        self.fixture
            .file_system()
            .write_all(&path, &bytes, case.options().clone())
            .map_err(|error| {
                ContractFailure::with_owned_source("conditional write first publication failed", error).at(id)
            })?;
        let observed = self.fixture.read_file(&path).map_err(|error| {
            ContractFailure::with_source("conditional write initial observation failed", error).at(id)
        })?;
        verify_condition(
            matches!(observed, FixtureSupport::Supported(actual) if actual == bytes),
            id,
            "conditional write first publication differs or evidence is unavailable",
        )?;
        let unexpected = super::write::bounded_payload(limit, b"unexpected", b'u');
        let failure = match self
            .fixture
            .file_system()
            .write_all(&path, &unexpected, case.options().clone())
        {
            Err(failure) => failure,
            Ok(_) => return Err(ContractFailure::message_only("conditional write replaced an existing target").at(id)),
        };
        let error = failure.error();
        if error.kind() != FsErrorKind::PreconditionFailed
            || !matches!(error.operation(), FsOperation::OpenWriter | FsOperation::CommitWriter)
            || error.path() != Some(&path)
            || error.provider() != Some(self.context.properties().info().provider_id())
        {
            return Err(ContractFailure::with_owned_source("conditional write rejection differs", failure).at(id));
        }
        let observed = self.fixture.read_file(&path).map_err(|error| {
            ContractFailure::with_source("conditional write rejection observation failed", error).at(id)
        })?;
        verify_condition(
            matches!(observed, FixtureSupport::Supported(actual) if actual == bytes),
            id,
            "conditional write rejection changed target bytes or evidence is unavailable",
        )?;
        Ok(ContractCheckOutcome::Passed)
    }
}
