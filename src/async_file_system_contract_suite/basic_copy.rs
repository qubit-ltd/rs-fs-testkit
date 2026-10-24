// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Independent basic copy and operation execution evidence.

use qubit_fs as qfs;
use qubit_fs::copy::CopyFailureState;
use qubit_fs::copy::CopyMethod;
use qubit_fs::copy::CopyOptions;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl AsyncFileSystemContractSuite<'_> {
    /// Executes one basic copy requirement using independently prepared
    /// contents.
    ///
    /// The check may create source and target paths, execute asynchronous
    /// filesystem I/O, and read files to observe the copied contents. It
    /// records the resulting check outcome in the suite context.
    ///
    /// # Parameters
    ///
    /// * `id` - Catalog identifier selecting the basic or repeated-execution
    ///   copy requirement.
    ///
    /// # Errors
    ///
    /// Returns a [`ContractFailure`] if preparation, copy execution, expected
    /// capability rejection, operation state, statistics, or observed contents
    /// do not satisfy the selected contract.
    pub(super) async fn check_basic_copy(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        self.context.begin(id.as_str());
        let source_relative = self.context.relative_name("copy-source");
        let target_relative = self.context.relative_name("copy-target");
        let target = self
            .fixture
            .path(&target_relative)
            .map_err(|error| ContractFailure::with_source("copy target preparation failed", error).at(id))?;
        self.context.record_created(target.clone());
        if self.check_copy_capability(id, &source_relative, &target).await? {
            return Ok(());
        }
        let bytes = b"copy bytes";
        let Some(case) = self
            .prepare_basic_copy(id, &source_relative, &target_relative, bytes)
            .await?
        else {
            return Ok(());
        };
        let (source, target, options) = case.into_parts();
        self.context.record_created(target.clone());
        verify_condition(
            options == CopyOptions::file(),
            id,
            "basic copy preparation changed required options",
        )?;
        self.context.record_created(source.clone());
        verify_condition(source != target, id, "copy source and target alias")?;
        self.verify_copy_contents(
            id,
            &source,
            bytes,
            "copy source observation failed",
            "copy source evidence is unavailable or differs",
        )
        .await?;
        self.execute_basic_copy(id, source, target, bytes).await
    }

    /// Handles the capability branch before any copy input is prepared.
    ///
    /// # Parameters
    ///
    /// * `id` - Catalog identifier for the copy check.
    /// * `source_relative` - Fixture-relative name for the source path.
    /// * `target` - Prepared target path used to probe capability rejection.
    ///
    /// # Returns
    ///
    /// Returns `true` when missing capabilities make the check complete or
    /// inapplicable, and `false` when copy preparation may proceed.
    ///
    /// # Errors
    ///
    /// Returns a [`ContractFailure`] if the expected capability rejection
    /// cannot be observed or has inconsistent error context.
    async fn check_copy_capability(
        &mut self,
        id: ContractCheckId,
        source_relative: &str,
        target: &qfs::path::Path,
    ) -> Result<bool, ContractFailure> {
        let missing = [
            FileSystemCapability::Read,
            FileSystemCapability::Write,
            FileSystemCapability::Copy,
        ]
        .into_iter()
        .find(|capability| !self.capable(*capability));
        if let Some(required) = missing {
            if id != ContractCheckId::CopyBasic {
                self.context.record_check(
                    id,
                    Some(FileSystemCapability::Copy),
                    ContractCheckOutcome::NotApplicable {
                        reason: "copy prerequisites are unavailable".to_owned(),
                    },
                );
                return Ok(true);
            }
            let source = self
                .fixture
                .path(source_relative)
                .map_err(|error| ContractFailure::with_source("copy source path preparation failed", error).at(id))?;
            let (failure, retained_operation) =
                match self
                    .fixture
                    .file_system()
                    .begin_copy(source.clone(), target.clone(), CopyOptions::file())
                {
                    Err(error) => (error, None),
                    Ok(mut operation) => match operation.execute().await {
                        Err(error) => (error, Some(operation)),
                        Ok(_) => {
                            return Err(ContractFailure::message_only("unsupported copy succeeded").at(id));
                        }
                    },
                };
            let path = if required == FileSystemCapability::Write {
                target
            } else {
                &source
            };
            let error = failure.error();
            if error.kind() != FsErrorKind::UnsupportedCapability
                || error.operation() != FsOperation::Copy
                || error.path() != Some(path)
                || error.target() != Some(target)
                || error.required_capability() != Some(required)
                || error.provider() != Some(self.context.properties().info().provider_id())
                || failure.state() != CopyFailureState::Unchanged
            {
                return Err(ContractFailure::with_owned_source(
                    "copy rejection context differs",
                    crate::ContractAsyncCopyFailure::new(failure, retained_operation),
                )
                .at(id));
            }
            self.context.record_check(
                id,
                Some(FileSystemCapability::Copy),
                ContractCheckOutcome::RejectedAsExpected,
            );
            return Ok(true);
        }
        Ok(false)
    }

    /// Prepares the independent source and target state for one copy check.
    ///
    /// # Parameters
    ///
    /// * `id` - Catalog identifier selecting the copy scenario to prepare.
    /// * `source_relative` - Fixture-relative name for the source file.
    /// * `target_relative` - Fixture-relative name for the target file.
    /// * `bytes` - Expected source contents supplied to the fixture.
    ///
    /// # Returns
    ///
    /// Returns the prepared case when the fixture can provide it, or `None`
    /// after recording an unavailable or inapplicable preparation.
    ///
    /// # Errors
    ///
    /// Returns a [`ContractFailure`] if the scenario is absent from the catalog
    /// or fixture preparation fails.
    async fn prepare_basic_copy(
        &mut self,
        id: ContractCheckId,
        source_relative: &str,
        target_relative: &str,
        bytes: &[u8],
    ) -> Result<Option<crate::CopyFixtureCase>, ContractFailure> {
        let scenario =
            crate::internal::check_catalog::required_copy_scenario(id, "copy preparation missing from catalog")?;
        let prepared = self
            .fixture
            .prepare_copy(scenario, source_relative, target_relative, bytes)
            .await
            .map_err(|error| ContractFailure::with_source("copy source preparation failed", error).at(id))?;
        let case = match prepared {
            crate::FixturePreparation::Ready(case) => case,
            crate::FixturePreparation::Unavailable { reason } | crate::FixturePreparation::NotApplicable { reason } => {
                self.context.record_check(
                    id,
                    Some(FileSystemCapability::Copy),
                    ContractCheckOutcome::Unverified { reason },
                );
                return Ok(None);
            }
        };
        Ok(Some(case))
    }

    /// Executes an admitted copy and verifies its outcome and retained state.
    ///
    /// # Parameters
    ///
    /// * `id` - Catalog identifier for the copy contract being checked.
    /// * `source` - Prepared source path to copy.
    /// * `target` - Prepared destination path.
    /// * `bytes` - Expected contents and byte count for the copy.
    ///
    /// # Errors
    ///
    /// Returns a [`ContractFailure`] if admission or execution fails, the
    /// operation state or statistics differ, or observed contents are
    /// incorrect.
    async fn execute_basic_copy(
        &mut self,
        id: ContractCheckId,
        source: qfs::path::Path,
        target: qfs::path::Path,
        bytes: &[u8],
    ) -> Result<(), ContractFailure> {
        let mut operation = self
            .fixture
            .file_system()
            .begin_copy(source.clone(), target.clone(), CopyOptions::file())
            .map_err(|error| {
                ContractFailure::with_owned_source(
                    "copy admission failed",
                    crate::ContractAsyncCopyFailure::new(error, None),
                )
                .at(id)
            })?;
        drop(operation.execute());
        verify_condition(
            operation.state() == qfs::copy::AsyncCopyOperationState::Ready && !operation.has_recovery(),
            id,
            "unpolled copy changed operation state",
        )?;
        let outcome = match operation.execute().await {
            Ok(outcome) => outcome,
            Err(error) => {
                return Err(ContractFailure::with_owned_source(
                    "copy execution failed",
                    crate::ContractAsyncCopyFailure::new(error, Some(operation)),
                )
                .at(id));
            }
        };
        verify_condition(
            outcome.stats().bytes == bytes.len() as u64 && outcome.stats().files + outcome.stats().objects == 1,
            id,
            "copy statistics differ",
        )?;
        verify_condition(
            outcome.used_fallback() == (outcome.method() == CopyMethod::Streamed),
            id,
            "copy method and fallback evidence disagree",
        )?;
        verify_condition(
            !self.fixture.copy_fallback_only() || outcome.method() == CopyMethod::Streamed,
            id,
            "fallback-only copy reported a native method",
        )?;
        verify_condition(
            operation.state() == qfs::copy::AsyncCopyOperationState::Completed,
            id,
            "successful copy did not complete the operation",
        )?;
        if id == ContractCheckId::CopyRepeatedExecute {
            for path in [&source, &target] {
                self.verify_copy_contents(
                    id,
                    path,
                    bytes,
                    "initial copy observation failed",
                    "initial copy contents differ or evidence is unavailable",
                )
                .await?;
            }
            let repeated = match operation.execute().await {
                Err(error) => error,
                Ok(_) => return Err(ContractFailure::message_only("copy executed twice").at(id)),
            };
            if repeated.error().kind() != FsErrorKind::InvalidState
                || repeated.state() != CopyFailureState::Published
                || repeated.partial_stats() != outcome.stats()
            {
                return Err(ContractFailure::with_owned_source(
                    "repeated copy lost completed facts",
                    crate::ContractAsyncCopyFailure::new(repeated, Some(operation)),
                )
                .at(id));
            }
            verify_condition(
                operation.state() == qfs::copy::AsyncCopyOperationState::Completed,
                id,
                "repeat changed completed copy state",
            )?;
        }
        for path in [&source, &target] {
            self.verify_copy_contents(
                id,
                path,
                bytes,
                "copy contents observation failed",
                "copy source or target contents differ or evidence is unavailable",
            )
            .await?;
        }
        self.context
            .record_check(id, Some(FileSystemCapability::Copy), ContractCheckOutcome::Passed);
        Ok(())
    }

    /// Reads one path and verifies that the fixture exposes the expected bytes.
    ///
    /// The asynchronous read is an observation side effect on the fixture and
    /// converts provider errors into failures associated with the active check.
    ///
    /// # Parameters
    ///
    /// * `id` - Catalog identifier for the active copy check.
    /// * `path` - Fixture path whose contents are observed.
    /// * `expected` - Bytes that the fixture is expected to return.
    /// * `read_error` - Failure message used when the fixture read fails.
    /// * `mismatch` - Failure message used when observed bytes do not match.
    ///
    /// # Errors
    ///
    /// Returns a [`ContractFailure`] if the read fails or the observed contents
    /// are unsupported or differ from `expected`.
    async fn verify_copy_contents(
        &self,
        id: ContractCheckId,
        path: &qfs::path::Path,
        expected: &[u8],
        read_error: &'static str,
        mismatch: &'static str,
    ) -> Result<(), ContractFailure> {
        let observed = self
            .fixture
            .read_file(path)
            .await
            .map_err(|error| ContractFailure::with_source(read_error, error).at(id))?;
        verify_condition(
            matches!(observed, FixtureSupport::Supported(actual) if actual == expected),
            id,
            mismatch,
        )
    }
}
