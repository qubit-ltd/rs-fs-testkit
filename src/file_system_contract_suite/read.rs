// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Executes individual read checks with independent typed preparation.

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::ResourceVersion;
use qubit_fs::read::ReadOptions;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FileSystemContract;
use crate::FileSystemContractSuite;
use crate::FixturePreparation;
use crate::FixtureSupport;
use crate::ReadScenario;
use crate::internal::check_catalog;
use crate::internal::limit_probe_plan::MAX_PROBE_BYTES;
use crate::internal::limit_probe_plan::finite_probe;
use crate::internal::read_expectations;
use crate::internal::verify_condition;
use crate::internal::verify_fs_error;

impl FileSystemContractSuite<'_> {
    /// Executes each registered read entry with its own request and setup.
    pub(super) fn check_read(&mut self) -> Result<(), ContractFailure> {
        self.context.begin("read");
        for spec in check_catalog::for_contract(FileSystemContract::Read, false) {
            self.check_read_item(spec.id)?;
        }
        Ok(())
    }

    /// Executes exactly one read check; it never completes another check.
    pub(super) fn check_read_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        self.context.begin(id.as_str());
        let spec = check_catalog::specification(id);
        let scenario = spec
            .read_scenario
            .ok_or_else(|| ContractFailure::message_only("selected entry is not a read check").at(id))?;
        let outcome = self.execute_read_scenario(id, scenario)?;
        self.context.record_check(id, spec.capability, outcome);
        Ok(())
    }

    /// Keeps observation and request failures attached to their actual entry.
    fn execute_read_scenario(
        &mut self,
        id: ContractCheckId,
        scenario: ReadScenario,
    ) -> Result<ContractCheckOutcome, ContractFailure> {
        let spec = check_catalog::specification(id);
        let capability = spec
            .capability
            .ok_or_else(|| ContractFailure::message_only("read catalog entry lacks a capability").at(id))?;
        let limit = self.context.properties().limits().max_read_range_bytes();
        let name = if scenario == ReadScenario::ChecksumCorruption {
            "checksum-failure".to_owned()
        } else {
            id.as_str().replace('/', "-")
        };
        let relative = self.context.relative_name(&name);
        let read_available = self.capable(FileSystemCapability::Read);
        if !read_available && id != ContractCheckId::ReadBasic {
            return Ok(ContractCheckOutcome::NotApplicable {
                reason: "Read capability is unavailable".to_owned(),
            });
        }
        if !read_available || !self.capable(capability) {
            if spec.optional {
                return Ok(ContractCheckOutcome::NotApplicable {
                    reason: format!("{capability:?} capability is unavailable"),
                });
            }
            let path = self
                .fixture
                .path(&relative)
                .map_err(|error| ContractFailure::with_source("read request path preparation failed", error).at(id))?;
            let options =
                read_expectations::options(scenario, limit, ResourceVersion::new("missing-capability-version"));
            let error = match self.fixture.file_system().open_reader(&path, options) {
                Err(error) => error,
                Ok(_) => {
                    return Err(
                        ContractFailure::message_only(format!("{id}: unavailable read request succeeded")).at(id),
                    );
                }
            };
            verify_fs_error(
                error,
                if read_available {
                    FsErrorKind::RequirementNotMet
                } else {
                    FsErrorKind::UnsupportedCapability
                },
                FsOperation::OpenReader,
                &path,
                self.context.properties().info().provider_id(),
                Some(capability),
                id,
            )?;
            return Ok(ContractCheckOutcome::RejectedAsExpected);
        }
        let boundary = finite_probe(limit, MAX_PROBE_BYTES);
        if scenario == ReadScenario::RangeLimit && boundary.is_none() {
            return Ok(ContractCheckOutcome::SkippedOptional {
                reason: "range limit is unknown, unbounded, inapplicable, or exceeds the bounded probe budget"
                    .to_owned(),
            });
        }
        let prepared = self
            .fixture
            .prepare_read(scenario, &relative, read_expectations::CONTENT)
            .map_err(|error| ContractFailure::with_source("read scenario preparation failed", error).at(id))?;
        let path = match prepared {
            FixturePreparation::Ready(path) => path,
            FixturePreparation::NotApplicable { reason } => {
                return Ok(ContractCheckOutcome::Unverified {
                    reason: format!("declared read capability requires scenario evidence: {reason}"),
                });
            }
            FixturePreparation::Unavailable { reason } => {
                return Ok(if spec.optional {
                    ContractCheckOutcome::SkippedOptional { reason }
                } else {
                    ContractCheckOutcome::Unverified { reason }
                });
            }
        };
        self.context.record_created(path.clone());
        let mut version = ResourceVersion::new("version-unused-by-request");
        if read_expectations::uses_version(scenario) {
            let current = self.fixture.resource_version(&path).map_err(|error| {
                ContractFailure::with_source("read current version observation failed", error).at(id)
            })?;
            let FixtureSupport::Supported(current) = current else {
                return Ok(ContractCheckOutcome::Unverified {
                    reason: "fixture current version unavailable".to_owned(),
                });
            };
            version = current.clone();
            if read_expectations::uses_stale(scenario) {
                let stale = self.fixture.stale_resource_version(&path).map_err(|error| {
                    ContractFailure::with_source("read stale version observation failed", error).at(id)
                })?;
                let FixtureSupport::Supported(stale) = stale else {
                    return Ok(ContractCheckOutcome::Unverified {
                        reason: "fixture stale version unavailable".to_owned(),
                    });
                };
                verify_condition(stale != current, id, "fixture stale version equals current version")?;
                version = stale;
            }
        }
        if scenario == ReadScenario::RangeLimit {
            let (maximum, over) =
                boundary.ok_or_else(|| ContractFailure::message_only("bounded range check has no boundary").at(id))?;
            let actual = self
                .fixture
                .file_system()
                .read_all(
                    &path,
                    ReadOptions::default().with_length(Some(maximum)),
                    maximum as usize,
                )
                .map_err(|error| {
                    ContractFailure::with_source("read at declared range boundary failed", error).at(id)
                })?;
            let expected = &read_expectations::CONTENT[..(maximum as usize).min(read_expectations::CONTENT.len())];
            verify_condition(
                actual == expected,
                id,
                "range boundary bytes differ from seeded content",
            )?;
            let error = match self
                .fixture
                .file_system()
                .open_reader(&path, ReadOptions::default().with_length(Some(over)))
            {
                Err(error) => error,
                Ok(_) => return Err(ContractFailure::message_only("read range limit was ignored").at(id)),
            };
            verify_fs_error(
                error,
                FsErrorKind::ResourceLimitExceeded,
                FsOperation::OpenReader,
                &path,
                self.context.properties().info().provider_id(),
                None,
                id,
            )?;
            return Ok(ContractCheckOutcome::Passed);
        }
        let options = read_expectations::options(scenario, limit, version);
        let result = self.fixture.file_system().read_all(&path, options, 64);
        if let Some(kind) = read_expectations::rejection(scenario) {
            let error = match result {
                Err(error) => error,
                Ok(_) => {
                    return Err(ContractFailure::message_only(format!(
                        "{id}: read accepted a request requiring rejection"
                    ))
                    .at(id));
                }
            };
            let operation = if kind == FsErrorKind::DataCorruption && error.operation() == FsOperation::Read {
                FsOperation::Read
            } else {
                FsOperation::OpenReader
            };
            verify_fs_error(
                error,
                kind,
                operation,
                &path,
                self.context.properties().info().provider_id(),
                None,
                id,
            )?;
            return Ok(ContractCheckOutcome::RejectedAsExpected);
        } else {
            let actual = result
                .map_err(|error| ContractFailure::with_source(format!("{id}: read request failed"), error).at(id))?;
            verify_condition(
                actual == read_expectations::bytes(scenario, limit),
                id,
                "read bytes differ from independent seed",
            )?;
        }
        if scenario == ReadScenario::Range {
            let eof = read_expectations::CONTENT.len() as u64;
            let length = limit.maximum().unwrap_or(1).min(1);
            for options in [
                ReadOptions::default().with_length(Some(0)),
                ReadOptions::default().with_offset(Some(eof)).with_length(Some(length)),
                ReadOptions::default()
                    .with_offset(Some(eof + 1))
                    .with_length(Some(length)),
            ] {
                let actual = self
                    .fixture
                    .file_system()
                    .read_all(&path, options, 64)
                    .map_err(|error| ContractFailure::with_source("empty range observation failed", error).at(id))?;
                verify_condition(actual.is_empty(), id, "empty or beyond-EOF range returned bytes")?;
            }
        }
        if scenario == ReadScenario::Basic {
            let error = match self.fixture.file_system().read_all(&path, ReadOptions::default(), 4) {
                Err(error) => error,
                Ok(_) => return Err(ContractFailure::message_only("read caller byte limit was ignored").at(id)),
            };
            verify_fs_error(
                error,
                FsErrorKind::ResourceLimitExceeded,
                FsOperation::Read,
                &path,
                self.context.properties().info().provider_id(),
                None,
                id,
            )?;
        }
        Ok(ContractCheckOutcome::Passed)
    }
}
