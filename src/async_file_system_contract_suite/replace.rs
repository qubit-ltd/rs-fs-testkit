// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Replacement and truncation evidence independent of basic creation.

use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::write::WriteDisposition;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixturePreparation;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl AsyncFileSystemContractSuite<'_> {
    /// Executes one replacement scenario and attributes preparation or I/O
    /// failures.
    pub(super) async fn check_replace(&mut self) -> Result<(), ContractFailure> {
        let id = ContractCheckId::WriteReplace;
        let outcome = self.execute_replace().await.map_err(|failure| failure.at(id))?;
        self.context
            .record_check(id, Some(FileSystemCapability::Write), outcome);
        Ok(())
    }

    /// Requires a longer seed so a missing truncation cannot masquerade as
    /// replacement.
    async fn execute_replace(&mut self) -> Result<ContractCheckOutcome, ContractFailure> {
        let id = ContractCheckId::WriteReplace;
        let relative = self.context.relative_name("write-replace");
        if !self.capable(FileSystemCapability::Write) {
            return Ok(ContractCheckOutcome::NotApplicable {
                reason: "Write capability is unavailable".to_owned(),
            });
        }
        let bytes = super::write::bounded_payload(self.context.properties().limits().max_write_bytes(), b"new", b'n');
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
            .map_err(|error| ContractFailure::with_source("replacement scenario preparation failed", error))?;
        let case = match prepared {
            FixturePreparation::Ready(case) => case,
            FixturePreparation::Unavailable { reason } | FixturePreparation::NotApplicable { reason } => {
                return Ok(ContractCheckOutcome::Unverified { reason });
            }
        };
        let path = case.path().clone();
        self.context.record_created(path.clone());
        verify_condition(
            case.bytes() == bytes && case.options().disposition() == WriteDisposition::CreateOrReplace,
            id,
            "replacement fixture changed request semantics",
        )?;
        let before = self
            .fixture
            .read_file(&path)
            .await
            .map_err(|error| ContractFailure::with_source("replacement initial observation failed", error))?;
        let FixtureSupport::Supported(expected) = before else {
            return Ok(ContractCheckOutcome::Unverified {
                reason: "replacement initial content unavailable".to_owned(),
            });
        };
        verify_condition(
            expected.len() > bytes.len(),
            id,
            "replacement fixture requires longer initial contents",
        )?;
        let publication = crate::internal::execute_write::execute_write(
            self.fixture.file_system(),
            path.clone(),
            bytes.clone(),
            case.options().clone(),
        )
        .await;
        let outcome =
            publication.map_err(|error| ContractFailure::with_owned_source("replacement publication failed", error))?;
        verify_condition(
            outcome.bytes_written().is_none_or(|count| count == bytes.len() as u64),
            id,
            "replacement byte count must describe only the replacement payload",
        )?;
        let observed = self
            .fixture
            .read_file(&path)
            .await
            .map_err(|error| ContractFailure::with_source("replacement publication observation failed", error))?;
        verify_condition(
            matches!(observed, FixtureSupport::Supported(actual) if actual == bytes),
            id,
            "write/replace: replacement did not publish exactly the requested bytes",
        )?;
        Ok(ContractCheckOutcome::Passed)
    }
}
