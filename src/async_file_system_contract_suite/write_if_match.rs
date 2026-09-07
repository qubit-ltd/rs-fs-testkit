// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Version-conditional replacement with independent version and byte evidence.

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::ResourceVersion;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteOptions;
use qubit_fs::write::WritePrecondition;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixturePreparation;
use crate::FixtureSupport;
use crate::internal::verify_condition;
use crate::internal::verify_fs_error;

impl AsyncFileSystemContractSuite<'_> {
    /// Records only the If-Match check, retaining original errors on failure.
    pub(super) async fn check_write_if_match(&mut self) -> Result<(), ContractFailure> {
        let outcome = self.execute_write_if_match().await?;
        self.context.record_check(
            ContractCheckId::WriteIfMatch,
            Some(FileSystemCapability::ConditionalWrite),
            outcome,
        );
        Ok(())
    }

    /// Publishes with the current version, rejects a stale one, and observes
    /// bytes.
    async fn execute_write_if_match(&mut self) -> Result<ContractCheckOutcome, ContractFailure> {
        let id = ContractCheckId::WriteIfMatch;
        let relative = self.context.relative_name("write-if-match");
        if !self.capable(FileSystemCapability::ConditionalWrite) {
            let path = self
                .fixture
                .path(&relative)
                .map_err(|error| ContractFailure::with_source("conditional write path failed", error).at(id))?;
            let options = WriteOptions::default()
                .with_precondition(WritePrecondition::IfMatch(ResourceVersion::new("missing-capability")));
            let error = match self.fixture.file_system().open_writer(&path, options).await {
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
            .await
            .map_err(|error| ContractFailure::with_source("If-Match preparation failed", error).at(id))?;
        let case = match prepared {
            FixturePreparation::Ready(case) => case,
            FixturePreparation::Unavailable { reason } | FixturePreparation::NotApplicable { reason } => {
                return Ok(ContractCheckOutcome::Unverified { reason });
            }
        };
        let path = case.path().clone();
        self.context.record_created(path.clone());
        let current = self.fixture.resource_version(&path).await.map_err(|error| {
            ContractFailure::with_source("If-Match current version observation failed", error).at(id)
        })?;
        let FixtureSupport::Supported(current) = current else {
            return Ok(ContractCheckOutcome::Unverified {
                reason: "fixture current version unavailable".to_owned(),
            });
        };
        verify_condition(
            case.bytes() == bytes
                && case.options().precondition() == &WritePrecondition::IfMatch(current)
                && case.options().disposition() == WriteDisposition::CreateOrReplace,
            id,
            "fixture changed If-Match request semantics",
        )?;
        let first = crate::internal::execute_write::execute_write(
            self.fixture.file_system(),
            path.clone(),
            bytes.clone(),
            case.options().clone(),
        )
        .await;
        first.map_err(|error| {
            ContractFailure::with_owned_source("conditional write first publication failed", error).at(id)
        })?;
        let observed = self.fixture.read_file(&path).await.map_err(|error| {
            ContractFailure::with_source("conditional write initial observation failed", error).at(id)
        })?;
        verify_condition(
            matches!(observed, FixtureSupport::Supported(actual) if actual == bytes),
            id,
            "conditional write first publication differs or evidence is unavailable",
        )?;
        let stale =
            self.fixture.stale_resource_version(&path).await.map_err(|error| {
                ContractFailure::with_source("If-Match stale version observation failed", error).at(id)
            })?;
        let now = self.fixture.resource_version(&path).await.map_err(|error| {
            ContractFailure::with_source("If-Match published version observation failed", error).at(id)
        })?;
        let (FixtureSupport::Supported(stale), FixtureSupport::Supported(now)) = (stale, now) else {
            return Ok(ContractCheckOutcome::Unverified {
                reason: "fixture stale or published version unavailable".to_owned(),
            });
        };
        verify_condition(stale != now, id, "fixture stale version equals published version")?;
        let retry_options = case
            .options()
            .clone()
            .with_precondition(WritePrecondition::IfMatch(stale));
        let unexpected = super::write::bounded_payload(limit, b"unexpected", b'u');
        let retry = crate::internal::execute_write::execute_write(
            self.fixture.file_system(),
            path.clone(),
            unexpected,
            retry_options,
        )
        .await;
        let failure = match retry {
            Err(failure) => failure,
            Ok(_) => return Err(ContractFailure::message_only("write/if-match: stale If-Match succeeded").at(id)),
        };
        let error = failure.error();
        if error.kind() != FsErrorKind::PreconditionFailed
            || !matches!(error.operation(), FsOperation::OpenWriter | FsOperation::CommitWriter)
            || error.path() != Some(&path)
            || error.provider() != Some(self.context.properties().info().provider_id())
        {
            return Err(ContractFailure::with_owned_source("conditional write rejection differs", failure).at(id));
        }
        let observed = self.fixture.read_file(&path).await.map_err(|error| {
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
