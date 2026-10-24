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

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FileSystemContractSuite;
use crate::FixturePreparation;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl FileSystemContractSuite<'_> {
    /// Runs conflict policies against a fresh source and pre-existing target.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after recording the outcome, or when copy prerequisites
    /// are unavailable; returns an error if preparation or verification fails.
    ///
    /// # Errors
    ///
    /// Returns a contract failure when fixture preparation, copy execution, or
    /// evidence verification fails.
    pub(super) fn check_copy_conflict(&mut self) -> Result<(), ContractFailure> {
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
        let bytes = b"copy bytes";
        let scenario =
            crate::internal::check_catalog::required_copy_scenario(id, "copy preparation missing from catalog")?;
        let prepared = self
            .fixture
            .prepare_copy(scenario, &source_relative, &target_relative, bytes)
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
        for (path, expected) in [(&source, bytes.as_slice()), (&target, b"existing".as_slice())] {
            let observed = self.fixture.read_file(path).map_err(|error| {
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
            self.check_copy_conflict_policy(id, &source, &target, bytes, policy, fallback)?;
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

    /// Executes one conflict policy and verifies its result and file evidence.
    ///
    /// # Parameters
    ///
    /// * `id` - Contract check associated with this policy execution.
    /// * `source` - Prepared source path whose contents must remain unchanged.
    /// * `target` - Existing destination path subject to the selected policy.
    /// * `bytes` - Expected source contents and overwrite payload.
    /// * `policy` - Conflict behavior to execute and verify.
    /// * `fallback` - Whether overwrite is expected to be rejected by fallback.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` when the result and observed file contents match the
    /// selected conflict behavior.
    ///
    /// # Errors
    ///
    /// Returns a contract failure when the policy result or resulting file
    /// contents differ from the expected conflict behavior.
    fn check_copy_conflict_policy(
        &mut self,
        id: ContractCheckId,
        source: &Path,
        target: &Path,
        bytes: &[u8],
        policy: CopyConflictPolicy,
        fallback: bool,
    ) -> Result<(), ContractFailure> {
        let options = CopyOptions::file().with_conflict(policy);
        let result = self.fixture.file_system().copy(source, target, options);
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
                || failure.state() != CopyFailureState::Unchanged
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
        for (path, expected) in [(source, bytes), (target, expected_target)] {
            let observed = self.fixture.read_file(path).map_err(|error| {
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
