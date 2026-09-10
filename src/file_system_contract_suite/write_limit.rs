// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Checks the write boundary using independent creation and observation.

use qubit_fs::error::FsErrorKind;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::write::WriteDisposition;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FileSystemContractSuite;
use crate::FixturePreparation;
use crate::FixtureSupport;
use crate::internal::limit_probe_plan::MAX_PROBE_BYTES;
use crate::internal::limit_probe_plan::finite_probe;
use crate::internal::verify_condition;

impl FileSystemContractSuite<'_> {
    /// Executes the boundary and successor requests, preserving typed failures.
    /// Prepared destinations remain in the suite cleanup ledger until teardown.
    pub(super) fn check_write_limit(&mut self) -> Result<(), ContractFailure> {
        let id = ContractCheckId::WriteLimit;
        let outcome = self.execute_write_limit()?;
        self.context
            .record_check(id, Some(FileSystemCapability::Write), outcome);
        Ok(())
    }

    /// Requires independently observed boundary bytes and a rejected successor.
    fn execute_write_limit(&mut self) -> Result<ContractCheckOutcome, ContractFailure> {
        let id = ContractCheckId::WriteLimit;
        let limit = self.context.properties().limits().max_write_bytes();
        let Some((maximum, over)) = finite_probe(limit, MAX_PROBE_BYTES) else {
            return Ok(ContractCheckOutcome::SkippedOptional {
                reason: "write limit is not finite or exceeds the bounded probe budget".to_owned(),
            });
        };
        for (name, count, reject) in [
            ("write-limit-boundary", maximum, false),
            ("write-limit-over", over, true),
        ] {
            let bytes = vec![b'a'; count as usize];
            let relative = self.context.relative_name(name);
            let prepared = self
                .fixture
                .prepare_write(
                    crate::internal::check_catalog::specification(id)
                        .write_scenario
                        .expect("write check has a preparation scenario"),
                    &relative,
                    &bytes,
                )
                .map_err(|error| ContractFailure::with_source("write limit preparation failed", error).at(id))?;
            let case = match prepared {
                FixturePreparation::Ready(case) => case,
                FixturePreparation::Unavailable { reason } => {
                    return Ok(ContractCheckOutcome::SkippedOptional { reason });
                }
                FixturePreparation::NotApplicable { reason } => {
                    return Ok(ContractCheckOutcome::Unverified {
                        reason: format!("finite Write capability requires boundary evidence: {reason}"),
                    });
                }
            };
            verify_condition(
                case.bytes() == bytes
                    && matches!(
                        case.options().disposition(),
                        WriteDisposition::CreateNew | WriteDisposition::CreateOrReplace
                    ),
                id,
                "write limit fixture changed creation semantics",
            )?;
            let path = case.path().clone();
            self.context.record_created(path.clone());
            let result = self
                .fixture
                .file_system()
                .write_all(&path, &bytes, case.options().clone());
            if reject {
                let failure = match result {
                    Err(failure) => failure,
                    Ok(_) => return Err(ContractFailure::message_only("write limit successor was accepted").at(id)),
                };
                let error = failure.error();
                if error.kind() != FsErrorKind::ResourceLimitExceeded
                    || error.path() != Some(&path)
                    || error.provider() != Some(self.context.properties().info().provider_id())
                {
                    return Err(ContractFailure::with_owned_source("write limit rejection differs", failure).at(id));
                }
                crate::internal::finish_expected_write_failure::finish_expected_write_failure(failure, id)?;
            } else {
                let outcome = result.map_err(|error| {
                    ContractFailure::with_owned_source("write boundary request failed", error).at(id)
                })?;
                verify_condition(
                    outcome.bytes_written().is_none_or(|count| count == maximum),
                    id,
                    "write boundary byte count differs",
                )?;
                let observed = self
                    .fixture
                    .read_file(&path)
                    .map_err(|error| ContractFailure::with_source("write boundary observation failed", error).at(id))?;
                verify_condition(
                    matches!(observed, FixtureSupport::Supported(actual) if actual == bytes),
                    id,
                    "write boundary published bytes differ or independent evidence is unavailable",
                )?;
            }
        }
        Ok(ContractCheckOutcome::Passed)
    }
}
