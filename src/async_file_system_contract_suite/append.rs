// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Append evidence based on independently observed initial contents.

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteOptions;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixturePreparation;
use crate::FixtureSupport;
use crate::internal::verify_condition;
use crate::internal::verify_fs_error;

impl AsyncFileSystemContractSuite<'_> {
    /// Executes one append scenario and attributes preparation or I/O failures.
    pub(super) async fn check_append(&mut self) -> Result<(), ContractFailure> {
        let id = ContractCheckId::AppendBasic;
        self.context.begin("append");
        let outcome = self.execute_append().await.map_err(|failure| failure.at(id))?;
        self.context
            .record_check(id, Some(FileSystemCapability::Append), outcome);
        Ok(())
    }

    /// Requires a nonempty seed so overwriting cannot masquerade as appending.
    async fn execute_append(&mut self) -> Result<ContractCheckOutcome, ContractFailure> {
        let id = ContractCheckId::AppendBasic;
        let relative = self.context.relative_name("append-target");
        if !self.capable(FileSystemCapability::Append) {
            let path = self
                .fixture
                .path(&relative)
                .map_err(|error| ContractFailure::with_source("append path preparation failed", error))?;
            let options = WriteOptions::default().with_disposition(WriteDisposition::Append);
            let error = match self.fixture.file_system().open_writer(&path, options).await {
                Err(error) => error,
                Ok(_) => return Err(ContractFailure::message_only("unavailable append request succeeded")),
            };
            verify_fs_error(
                error,
                FsErrorKind::RequirementNotMet,
                FsOperation::OpenWriter,
                &path,
                self.context.properties().info().provider_id(),
                Some(FileSystemCapability::Append),
                id,
            )?;
            return Ok(ContractCheckOutcome::RejectedAsExpected);
        }
        let bytes =
            super::write::bounded_payload(self.context.properties().limits().max_write_bytes(), b"-after", b'a');
        let prepared = self
            .fixture
            .prepare_write(
                crate::internal::check_catalog::specification(id)
                    .write_scenario
                    .expect("write check has a preparation scenario"),
                &relative,
                &bytes,
            )
            .await
            .map_err(|error| ContractFailure::with_source("append scenario preparation failed", error))?;
        let case = match prepared {
            FixturePreparation::Ready(case) => case,
            FixturePreparation::Unavailable { reason } | FixturePreparation::NotApplicable { reason } => {
                return Ok(ContractCheckOutcome::Unverified { reason });
            }
        };
        let path = case.path().clone();
        self.context.record_created(path.clone());
        verify_condition(
            case.bytes() == bytes && case.options().disposition() == WriteDisposition::Append,
            id,
            "append fixture changed request semantics",
        )?;
        let before = self
            .fixture
            .read_file(&path)
            .await
            .map_err(|error| ContractFailure::with_source("append initial observation failed", error))?;
        let FixtureSupport::Supported(mut expected) = before else {
            return Ok(ContractCheckOutcome::Unverified {
                reason: "append initial content unavailable".to_owned(),
            });
        };
        verify_condition(
            !expected.is_empty(),
            id,
            "append fixture requires nonempty initial contents",
        )?;
        expected.extend_from_slice(&bytes);
        let publication = crate::internal::execute_write::execute_write(
            self.fixture.file_system(),
            path.clone(),
            bytes.clone(),
            case.options().clone(),
        )
        .await;
        let outcome =
            publication.map_err(|error| ContractFailure::with_owned_source("append publication failed", error))?;
        verify_condition(
            outcome.bytes_written().is_none_or(|count| count == bytes.len() as u64),
            id,
            "append byte count must describe only the appended payload",
        )?;
        let observed = self
            .fixture
            .read_file(&path)
            .await
            .map_err(|error| ContractFailure::with_source("append publication observation failed", error))?;
        verify_condition(
            matches!(observed, FixtureSupport::Supported(actual) if actual == expected),
            id,
            "append/basic: initial or appended bytes were not retained",
        )?;
        Ok(ContractCheckOutcome::Passed)
    }
}
