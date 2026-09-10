// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Implements writer and publication contracts.

use qubit_fs::metadata::FileSystemLimit;

use super::*;
use crate::ContractCheckId;

impl<'a> FileSystemContractSuite<'a> {
    /// Returns ordinary errors without dropping original recovery failures.
    pub(super) fn check_write(&mut self) -> Result<(), crate::ContractFailure> {
        for spec in crate::internal::check_catalog::for_contract(FileSystemContract::Write, false) {
            self.check_write_item(spec.id)?;
        }
        Ok(())
    }

    /// Executes exactly one write check with its own applicability decision.
    pub(super) fn check_write_item(&mut self, id: ContractCheckId) -> Result<(), crate::ContractFailure> {
        self.context.begin(id.as_str());
        let spec = crate::internal::check_catalog::specification(id);
        if spec.contract != FileSystemContract::Write {
            return Err(crate::ContractFailure::message_only("selected entry is not a write check").at(id));
        }
        if !self.capable(FileSystemCapability::Write)
            && !matches!(id, ContractCheckId::WriteBasic | ContractCheckId::WriteOwningOperation)
        {
            self.context.record_check(
                id,
                spec.capability,
                ContractCheckOutcome::NotApplicable {
                    reason: "Write capability is unavailable".to_owned(),
                },
            );
            return Ok(());
        }
        match id {
            ContractCheckId::WriteBasic => self.check_write_basic(),
            ContractCheckId::WriteCreateConflict => self.check_create_conflict(),
            ContractCheckId::WriteReplace => self.check_replace(),
            ContractCheckId::WriteAbort => self.check_abort(),
            ContractCheckId::WriteLimit => self.check_write_limit(),
            ContractCheckId::WriteIfAbsent => self.check_write_if_absent(),
            ContractCheckId::WriteIfMatch => self.check_write_if_match(),
            ContractCheckId::AppendBasic => self.check_append(),
            ContractCheckId::WriteAtomicReplaceExisting | ContractCheckId::WriteDurable => {
                self.check_write_guarantee(id)
            }
            _ => Err(crate::ContractFailure::message_only("write check is unavailable in this execution mode").at(id)),
        }
    }

    /// Records creation evidence without gating independently prepared checks.
    pub(super) fn check_write_basic(&mut self) -> Result<(), crate::ContractFailure> {
        let id = ContractCheckId::WriteBasic;
        if !self.capable(FileSystemCapability::Write) {
            let relative = self.context.relative_name("write-unavailable");
            let path = self.fixture.path(&relative).map_err(|error| {
                crate::ContractFailure::with_source("write/basic: path preparation failed", error).at(id)
            })?;
            let error = match self.fixture.file_system().open_writer(&path, Default::default()) {
                Ok(_) => {
                    return Err(crate::ContractFailure::message_only("write/basic: unavailable writer opened").at(id));
                }
                Err(error) => error,
            };
            crate::internal::verify_open_failure(
                error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::OpenWriter,
                &path,
                self.context.properties().info().provider_id(),
                Some(FileSystemCapability::Write),
                id,
            )?;
            self.context.record_check(
                ContractCheckId::WriteBasic,
                Some(FileSystemCapability::Write),
                ContractCheckOutcome::RejectedAsExpected,
            );
            return Ok(());
        }
        let limit = self.context.properties().limits().max_write_bytes();
        let initial = bounded_payload(limit, b"written", b'x');
        let relative = self.context.relative_name("write-create");
        let prepared = self
            .fixture
            .prepare_write(
                crate::internal::check_catalog::specification(id)
                    .write_scenario
                    .expect("write check has a preparation scenario"),
                &relative,
                &initial,
            )
            .map_err(|error| {
                crate::ContractFailure::with_source("write/basic: scenario preparation failed", error).at(id)
            })?;
        let case = match prepared {
            crate::FixturePreparation::Ready(case) => case,
            crate::FixturePreparation::NotApplicable { reason } | crate::FixturePreparation::Unavailable { reason } => {
                self.context.record_check(
                    ContractCheckId::WriteBasic,
                    Some(FileSystemCapability::Write),
                    ContractCheckOutcome::Unverified {
                        reason: format!("declared Write requires a positive scenario: {reason}"),
                    },
                );
                return Ok(());
            }
        };
        if case.bytes() != initial.as_slice()
            || !matches!(
                case.options().disposition(),
                WriteDisposition::CreateNew | WriteDisposition::CreateOrReplace
            )
        {
            return Err(crate::ContractFailure::message_only(
                "write/basic: fixture changed the requested creation semantics",
            )
            .at(id));
        }
        let path = case.path().clone();
        self.context.record_created(path.clone());
        let outcome = self
            .fixture
            .file_system()
            .write_all(&path, &initial, case.options().clone())
            .map_err(|error| crate::ContractFailure::with_owned_source("write/basic: write failed", error).at(id))?;
        if outcome
            .bytes_written()
            .is_some_and(|count| count != initial.len() as u64)
        {
            return Err(crate::ContractFailure::message_only("write/basic: published byte count mismatch").at(id));
        }
        let observed = self.fixture.read_file(&path).map_err(|error| {
            crate::ContractFailure::with_source("write/basic: fixture observation failed", error).at(id)
        })?;
        match observed {
            FixtureSupport::Supported(bytes) if bytes == initial => {}
            FixtureSupport::Supported(_) => {
                return Err(crate::ContractFailure::message_only(
                    "write/basic: writer contract: write was not published",
                )
                .at(id));
            }
            FixtureSupport::Unsupported => {
                return Err(
                    crate::ContractFailure::message_only("write/basic: fixture.read_file support is required").at(id),
                );
            }
        }
        self.context.record_check(
            ContractCheckId::WriteBasic,
            Some(FileSystemCapability::Write),
            ContractCheckOutcome::Passed,
        );
        Ok(())
    }
}

/// Selects a bounded write payload that fits the provider's advertised limit.
///
/// Unknown, inapplicable, and unbounded dimensions use the preferred payload.
/// A finite limit truncates the payload without ever allocating beyond the
/// preferred test vector; zero permits an empty publication probe.
pub(super) fn bounded_payload(limit: FileSystemLimit, preferred: &[u8], fill: u8) -> Vec<u8> {
    let length = limit
        .maximum()
        .map_or(preferred.len() as u64, |maximum| maximum.min(preferred.len() as u64)) as usize;
    if length == preferred.len() {
        preferred.to_vec()
    } else {
        vec![fill; length]
    }
}
