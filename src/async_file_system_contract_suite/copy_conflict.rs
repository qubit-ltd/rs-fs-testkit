// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Independent copy destination-conflict and overwrite evidence.

use qubit_fs::copy::CopyConflictPolicy;
use qubit_fs::copy::CopyFailureState;
use qubit_fs::copy::CopyOptions;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::path::Path;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixturePreparation;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl AsyncFileSystemContractSuite<'_> {
    /// Runs copy conflict policies against a fresh source and pre-existing
    /// target.
    ///
    /// This prepares the fixture, performs provider I/O for each policy,
    /// verifies error metadata and resulting file contents, and records the
    /// check outcome in the suite context.
    ///
    /// # Errors
    ///
    /// Returns a [`ContractFailure`] when fixture preparation or observation
    /// fails, or when the provider's response violates the expected contract.
    /// The failure retains the check identifier and the underlying provider
    /// failure when one is available.
    ///
    /// # Panics
    ///
    /// Panics if the `Fail` policy reaches the success branch, which would
    /// contradict the earlier rejection check.
    pub(super) async fn check_copy_conflict(&mut self) -> Result<(), ContractFailure> {
        let id = ContractCheckId::CopyFallbackOverwriteRejected;
        self.context.begin(id.as_str());
        if [
            FileSystemCapability::Copy,
            FileSystemCapability::Read,
            FileSystemCapability::Write,
        ]
        .into_iter()
        .any(|capability| !self.capable(capability))
        {
            self.context.record_check(
                id,
                Some(FileSystemCapability::Copy),
                ContractCheckOutcome::NotApplicable {
                    reason: "copy prerequisites are unavailable".to_owned(),
                },
            );
            return Ok(());
        }
        let source_relative = self.context.relative_name("copy-conflict-source");
        let target_relative = self.context.relative_name("copy-conflict-target");
        let bytes = crate::internal::limit_probe_plan::bounded_payload(
            b"copy bytes",
            self.context.properties().limits().max_write_bytes(),
        );
        let scenario =
            crate::internal::check_catalog::required_copy_scenario(id, "copy preparation missing from catalog")?;
        let prepared = self
            .fixture
            .prepare_copy(scenario, &source_relative, &target_relative, bytes)
            .await
            .map_err(|error| ContractFailure::with_source("copy conflict preparation failed", error).at(id))?;
        let case = match prepared {
            FixturePreparation::Ready(case) => case,
            FixturePreparation::Unavailable { reason } | FixturePreparation::NotApplicable { reason } => {
                self.context.record_check(
                    id,
                    Some(FileSystemCapability::Copy),
                    ContractCheckOutcome::Unverified { reason },
                );
                return Ok(());
            }
        };
        let (source, target, options) = case.into_parts();
        self.context.record_created(source.clone());
        self.context.record_created(target.clone());
        verify_condition(
            source != target && options == CopyOptions::file(),
            id,
            "copy conflict preparation changed required paths or options",
        )?;
        for (path, expected) in [(&source, bytes), (&target, b"existing".as_slice())] {
            let observed = self.fixture.read_file(path).await.map_err(|error| {
                ContractFailure::with_source("copy conflict initial observation failed", error).at(id)
            })?;
            verify_condition(
                matches!(observed, FixtureSupport::Supported(actual) if actual == expected),
                id,
                "copy conflict initial contents differ or evidence is unavailable",
            )?;
        }
        let fallback = self.fixture.copy_fallback_only();
        for policy in [
            CopyConflictPolicy::Fail,
            CopyConflictPolicy::Skip,
            CopyConflictPolicy::Overwrite,
        ] {
            self.check_copy_conflict_policy(id, &source, &target, bytes, policy, fallback)
                .await?;
        }
        self.context.record_check(
            id,
            Some(FileSystemCapability::Copy),
            if fallback {
                ContractCheckOutcome::RejectedAsExpected
            } else {
                ContractCheckOutcome::Passed
            },
        );
        Ok(())
    }

    /// Executes one conflict policy and verifies both its outcome and file
    /// evidence.
    ///
    /// # Errors
    ///
    /// Returns a contract failure when the policy result or resulting file
    /// contents differ from the expected conflict behavior.
    ///
    /// # Parameters
    ///
    /// * `id` identifies the contract check for failure reporting.
    /// * `source` and `target` identify the prepared files whose conflict is
    ///   tested.
    /// * `bytes` are the expected source contents and overwrite payload.
    /// * `policy` selects the conflict behavior to execute.
    /// * `fallback` indicates whether overwrite is implemented through the
    ///   fallback path.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` when the policy outcome and file contents match the
    /// contract, or `ContractFailure` when execution or verification fails.
    async fn check_copy_conflict_policy(
        &mut self,
        id: ContractCheckId,
        source: &Path,
        target: &Path,
        bytes: &[u8],
        policy: CopyConflictPolicy,
        fallback: bool,
    ) -> Result<(), ContractFailure> {
        let options = CopyOptions::file().with_conflict(policy);
        let result = crate::internal::execute_copy::execute_copy(
            self.fixture.file_system(),
            source.clone(),
            target.clone(),
            options,
        )
        .await;
        let rejected_kind = match policy {
            CopyConflictPolicy::Fail => Some(FsErrorKind::AlreadyExists),
            CopyConflictPolicy::Overwrite if fallback => Some(FsErrorKind::RequirementNotMet),
            _ => None,
        };
        if let Some(kind) = rejected_kind {
            let failure = match result {
                Err(failure) => failure,
                Ok(_) => {
                    return Err(ContractFailure::message_only("copy conflict unexpectedly succeeded").at(id));
                }
            };
            let error = failure.error();
            if error.kind() != kind
                || error.operation() != FsOperation::Copy
                || (error.path() != Some(source) && error.path() != Some(target))
                || error.target() != Some(target)
                || error.provider() != Some(self.context.properties().info().provider_id())
                || failure.failure().state() != CopyFailureState::Unchanged
            {
                return Err(ContractFailure::with_owned_source("copy conflict rejection differs", failure).at(id));
            }
        } else {
            let outcome = result
                .map_err(|error| ContractFailure::with_owned_source("copy conflict execution failed", error).at(id))?;
            match policy {
                CopyConflictPolicy::Skip => {
                    verify_condition(outcome.stats().skipped == 1, id, "copy skip statistics differ")?
                }
                CopyConflictPolicy::Overwrite => verify_condition(
                    outcome.stats().overwritten == 1 && outcome.stats().bytes == bytes.len() as u64,
                    id,
                    "copy overwrite statistics differ",
                )?,
                CopyConflictPolicy::Fail => unreachable!("failure policy handled as rejection"),
            }
        }
        let expected_target = if policy == CopyConflictPolicy::Overwrite && !fallback {
            bytes
        } else {
            b"existing"
        };
        self.verify_copy_conflict_result_contents(id, source, target, bytes, expected_target)
            .await
    }

    /// Confirms the policy preserved the source and left the expected target
    /// bytes.
    ///
    /// # Errors
    ///
    /// Returns a contract failure when either file cannot be observed or its
    /// contents differ from the expected bytes.
    ///
    /// # Parameters
    ///
    /// * `id` identifies the contract check for failure reporting.
    /// * `source` and `target` are the files whose contents are observed.
    /// * `source_bytes` and `target_bytes` are their respective expected
    ///   contents.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` when both files match their expected contents, or
    /// `ContractFailure` when observation or verification fails.
    async fn verify_copy_conflict_result_contents(
        &mut self,
        id: ContractCheckId,
        source: &Path,
        target: &Path,
        source_bytes: &[u8],
        target_bytes: &[u8],
    ) -> Result<(), ContractFailure> {
        for (path, expected) in [(source, source_bytes), (target, target_bytes)] {
            let observed = self.fixture.read_file(path).await.map_err(|error| {
                ContractFailure::with_source("copy conflict result observation failed", error).at(id)
            })?;
            verify_condition(
                matches!(observed, FixtureSupport::Supported(actual) if actual == expected),
                id,
                "copy conflict changed source or published incorrect target contents",
            )?;
        }
        Ok(())
    }
}
