// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Version-conditional replacement with independent version and byte evidence.

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::metadata::ResourceVersion;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteOptions;
use qubit_fs::write::WritePrecondition;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FileSystemContractSuite;
use crate::FixturePreparation;
use crate::FixtureSupport;
use crate::WriteFixtureCase;
use crate::internal::verify_condition;
use crate::internal::verify_open_failure;

impl FileSystemContractSuite<'_> {
    /// Records only the If-Match check, retaining original errors on failure.
    ///
    /// # Returns
    ///
    /// `Ok(())` when the check outcome has been recorded.
    ///
    /// # Errors
    ///
    /// Returns the `ContractFailure` produced when the conditional write
    /// request or its observed result violates the contract.
    pub(super) fn check_write_if_match(&mut self) -> Result<(), ContractFailure> {
        let outcome = self.execute_write_if_match()?;
        self.context.record_check(
            ContractCheckId::WriteIfMatch,
            Some(FileSystemCapability::ConditionalWrite),
            outcome,
        );
        Ok(())
    }

    /// Runs the version-conditional replacement contract using independent
    /// fixture setup and observations.
    ///
    /// # Returns
    ///
    /// `Passed` when current-version publication succeeds and stale-version
    /// publication is rejected without changing the retained bytes;
    /// `RejectedAsExpected` when the provider lacks conditional-write support;
    /// or `Unverified` when the fixture cannot provide required setup or
    /// version evidence.
    ///
    /// # Errors
    ///
    /// Returns a `ContractFailure` if fixture setup, either publication, or
    /// any observed request, rejection, or retained bytes violate the
    /// If-Match contract.
    ///
    /// # Panics
    ///
    /// Panics if the write-check catalog entry has no preparation scenario.
    fn execute_write_if_match(&mut self) -> Result<ContractCheckOutcome, ContractFailure> {
        let relative = self.context.relative_name("write-if-match");
        if !self.capable(FileSystemCapability::ConditionalWrite) {
            return self.reject_write_if_match_without_capability(&relative);
        }
        let limit = self.context.properties().limits().max_write_bytes();
        let bytes = super::write::bounded_payload(limit, b"conditional", b'c');
        let prepared = self.prepare_if_match_case(&relative, &bytes)?;
        let case = match prepared {
            FixturePreparation::Ready(case) => case,
            FixturePreparation::Unavailable { reason } | FixturePreparation::NotApplicable { reason } => {
                return Ok(ContractCheckOutcome::Unverified { reason });
            }
        };
        let path = case.path().clone();
        self.context.record_created(path.clone());
        if self.publish_with_current_version(&case, &path, &bytes)?.is_none() {
            return Ok(ContractCheckOutcome::Unverified {
                reason: "fixture current version unavailable".to_owned(),
            });
        }
        self.verify_stale_if_match_rejection(&case, &path, &bytes, limit)
    }

    /// Prepares the independent fixture request for the If-Match scenario.
    ///
    /// # Errors
    ///
    /// Returns a contract failure when fixture preparation fails.
    fn prepare_if_match_case(
        &self,
        relative: &str,
        bytes: &[u8],
    ) -> Result<FixturePreparation<WriteFixtureCase>, ContractFailure> {
        let id = ContractCheckId::WriteIfMatch;
        self.fixture
            .prepare_write(
                crate::internal::check_catalog::specification(id)
                    .write_scenario
                    .expect("write check has a preparation scenario"),
                relative,
                bytes,
            )
            .map_err(|error| ContractFailure::with_source("If-Match preparation failed", error).at(id))
    }

    /// Publishes using the fixture's current version and verifies retained
    /// bytes.
    ///
    /// Returns `None` when the fixture cannot independently observe a current
    /// version, allowing the caller to record an unverified outcome.
    ///
    /// # Errors
    ///
    /// Returns a contract failure when request semantics, publication, or the
    /// resulting bytes differ from the prepared request.
    fn publish_with_current_version(
        &mut self,
        case: &WriteFixtureCase,
        path: &qubit_fs::path::Path,
        bytes: &[u8],
    ) -> Result<Option<()>, ContractFailure> {
        let id = ContractCheckId::WriteIfMatch;
        let current = self.fixture.resource_version(path).map_err(|error| {
            ContractFailure::with_source("If-Match current version observation failed", error).at(id)
        })?;
        let FixtureSupport::Supported(current) = current else {
            return Ok(None);
        };
        verify_condition(
            case.bytes() == bytes
                && case.options().precondition() == &WritePrecondition::IfMatch(current)
                && case.options().disposition() == WriteDisposition::CreateOrReplace,
            id,
            "fixture changed If-Match request semantics",
        )?;
        self.fixture
            .file_system()
            .write_all(path, bytes, case.options().clone())
            .map_err(|error| {
                ContractFailure::with_owned_source("conditional write first publication failed", error).at(id)
            })?;
        let observed = self.fixture.read_file(path).map_err(|error| {
            ContractFailure::with_source("conditional write initial observation failed", error).at(id)
        })?;
        verify_condition(
            matches!(observed, FixtureSupport::Supported(actual) if actual == bytes),
            id,
            "conditional write first publication differs or evidence is unavailable",
        )?;
        Ok(Some(()))
    }

    /// Rejects a write using an independently observed stale version and
    /// confirms the successful publication remains intact.
    ///
    /// Returns `Unverified` when stale or published version evidence is
    /// unavailable.
    ///
    /// # Errors
    ///
    /// Returns a contract failure when version evidence, rejection metadata,
    /// or bytes after rejection violate the If-Match contract.
    fn verify_stale_if_match_rejection(
        &mut self,
        case: &WriteFixtureCase,
        path: &qubit_fs::path::Path,
        bytes: &[u8],
        limit: FileSystemLimit,
    ) -> Result<ContractCheckOutcome, ContractFailure> {
        let id = ContractCheckId::WriteIfMatch;
        let stale = self
            .fixture
            .stale_resource_version(path)
            .map_err(|error| ContractFailure::with_source("If-Match stale version observation failed", error).at(id))?;
        let now = self.fixture.resource_version(path).map_err(|error| {
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
        let failure = match self.fixture.file_system().write_all(path, &unexpected, retry_options) {
            Err(failure) => failure,
            Ok(_) => {
                return Err(ContractFailure::message_only("write/if-match: stale If-Match succeeded").at(id));
            }
        };
        let error = failure.error();
        if error.kind() != FsErrorKind::PreconditionFailed
            || !matches!(error.operation(), FsOperation::OpenWriter | FsOperation::CommitWriter)
            || error.path() != Some(path)
            || error.provider() != Some(self.context.properties().info().provider_id())
        {
            return Err(ContractFailure::with_owned_source("conditional write rejection differs", failure).at(id));
        }
        crate::internal::finish_expected_write_failure::finish_expected_write_failure(failure, id)?;
        let observed = self.fixture.read_file(path).map_err(|error| {
            ContractFailure::with_source("conditional write rejection observation failed", error).at(id)
        })?;
        verify_condition(
            matches!(observed, FixtureSupport::Supported(actual) if actual == bytes),
            id,
            "conditional write rejection changed target bytes or evidence is unavailable",
        )?;
        Ok(ContractCheckOutcome::Passed)
    }

    /// Confirms that a provider without conditional-write support rejects an
    /// If-Match request before opening a writer.
    fn reject_write_if_match_without_capability(
        &self,
        relative: &str,
    ) -> Result<ContractCheckOutcome, ContractFailure> {
        let id = ContractCheckId::WriteIfMatch;
        let path = self
            .fixture
            .path(relative)
            .map_err(|error| ContractFailure::with_source("conditional write path failed", error).at(id))?;
        let options = WriteOptions::default()
            .with_precondition(WritePrecondition::IfMatch(ResourceVersion::new("missing-capability")));
        let error = match self.fixture.file_system().open_writer(&path, options) {
            Err(error) => error,
            Ok(_) => {
                return Err(ContractFailure::message_only("unavailable conditional write succeeded").at(id));
            }
        };
        verify_open_failure(
            error,
            FsErrorKind::RequirementNotMet,
            FsOperation::OpenWriter,
            &path,
            self.context.properties().info().provider_id(),
            Some(FileSystemCapability::ConditionalWrite),
            id,
        )?;
        Ok(ContractCheckOutcome::RejectedAsExpected)
    }
}
