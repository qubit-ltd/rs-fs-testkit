// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Explicit abort with independent publication evidence and retained recovery.

use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::write::WriteAbortOutcome;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriterState;
use qubit_io::Output;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::ContractWriterFailure;
use crate::FileSystemContractSuite;
use crate::FixturePreparation;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl FileSystemContractSuite<'_> {
    /// Executes one abort check and retains the writer if streaming or cleanup
    /// fails.
    pub(super) fn check_abort(&mut self) -> Result<(), ContractFailure> {
        let id = ContractCheckId::WriteAbort;
        let outcome = self.execute_abort().map_err(|failure| failure.at(id))?;
        self.context
            .record_check(id, Some(FileSystemCapability::Write), outcome);
        Ok(())
    }

    /// Checks the abort outcome against state and independent target existence.
    fn execute_abort(&mut self) -> Result<ContractCheckOutcome, ContractFailure> {
        let id = ContractCheckId::WriteAbort;
        if !self.capable(FileSystemCapability::Write) {
            return Ok(ContractCheckOutcome::NotApplicable {
                reason: "Write capability is unavailable".to_owned(),
            });
        }
        let bytes =
            super::write::bounded_payload(self.context.properties().limits().max_write_bytes(), b"aborted", b'a');
        let relative = self.context.relative_name("write-aborted");
        let prepared = self
            .fixture
            .prepare_write(
                crate::internal::check_catalog::specification(id)
                    .write_scenario
                    .expect("write check has a preparation scenario"),
                &relative,
                &bytes,
            )
            .map_err(|error| ContractFailure::with_source("abort preparation failed", error))?;
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
            "abort fixture changed creation semantics",
        )?;
        let before = self
            .fixture
            .exists_out_of_band(&path)
            .map_err(|error| ContractFailure::with_source("abort initial observation failed", error))?;
        let FixtureSupport::Supported(before) = before else {
            return Ok(ContractCheckOutcome::Unverified {
                reason: "abort initial existence evidence unavailable".to_owned(),
            });
        };
        verify_condition(!before, id, "abort fixture requires an absent destination")?;
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&path, case.options().clone())
            .map_err(|error| ContractFailure::with_owned_source("abort writer opening failed", error))?;
        if let Err(error) = Output::write_fully(&mut writer, &bytes) {
            return Err(ContractFailure::with_owned_source(
                "abort writer rejected bytes",
                ContractWriterFailure::new(error, writer),
            ));
        }
        let outcome = match writer.abort() {
            Ok(outcome) => outcome,
            Err(error) => {
                return Err(ContractFailure::with_owned_source(
                    "explicit writer abort failed",
                    ContractWriterFailure::new(error, writer),
                ));
            }
        };
        let expected_state = match outcome {
            WriteAbortOutcome::NotPublished => WriterState::Aborted,
            WriteAbortOutcome::Published => WriterState::Published,
            WriteAbortOutcome::Indeterminate => WriterState::Indeterminate,
        };
        verify_condition(
            writer.state() == expected_state,
            id,
            "abort outcome and writer state disagree",
        )?;
        let after = self
            .fixture
            .exists_out_of_band(&path)
            .map_err(|error| ContractFailure::with_source("abort final observation failed", error))?;
        let FixtureSupport::Supported(after) = after else {
            return Ok(ContractCheckOutcome::Unverified {
                reason: "abort final existence evidence unavailable".to_owned(),
            });
        };
        match outcome {
            WriteAbortOutcome::NotPublished => {
                verify_condition(!after, id, "abort claimed NotPublished but created the destination")?
            }
            WriteAbortOutcome::Published => {
                verify_condition(after, id, "abort claimed Published but destination is absent")?
            }
            WriteAbortOutcome::Indeterminate => {}
        }
        Ok(ContractCheckOutcome::Passed)
    }
}
