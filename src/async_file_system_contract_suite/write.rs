// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements writer and publication contracts.

use qubit_fs::metadata::FileSystemLimit;

use super::*;
use crate::ContractCheckId;

impl<'a> AsyncFileSystemContractSuite<'a> {
    /// Returns ordinary errors without dropping original recovery failures.
    pub(super) async fn check_write(&mut self) -> Result<(), crate::ContractFailure> {
        for spec in crate::internal::check_catalog::for_contract(FileSystemContract::Write, true) {
            self.check_write_item(spec.id).await?;
        }
        Ok(())
    }

    /// Executes exactly one write check with its own applicability decision.
    pub(super) async fn check_write_item(&mut self, id: ContractCheckId) -> Result<(), crate::ContractFailure> {
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
            ContractCheckId::WriteBasic => self.check_write_basic().await,
            ContractCheckId::WriteCreateConflict => self.check_create_conflict().await,
            ContractCheckId::WriteReplace => self.check_replace().await,
            ContractCheckId::WriteAbort => self.check_abort().await,
            ContractCheckId::WriteLimit => self.check_write_limit().await,
            ContractCheckId::WriteIfAbsent => self.check_write_if_absent().await,
            ContractCheckId::WriteIfMatch => self.check_write_if_match().await,
            ContractCheckId::AppendBasic => self.check_append().await,
            ContractCheckId::WriteAtomicReplaceExisting | ContractCheckId::WriteDurable => {
                self.check_write_guarantee(id).await
            }
            ContractCheckId::WriteOwningOperation | ContractCheckId::WriteRepeatedExecute => {
                self.check_owning_write(id).await
            }
            ContractCheckId::WriteCancelOpen
            | ContractCheckId::WriteCancelWrite
            | ContractCheckId::WriteCancelFlush
            | ContractCheckId::WriteCancelCommit => self.check_write_cancellation_item(id).await,
            _ => Err(crate::ContractFailure::message_only("write check is unavailable in this execution mode").at(id)),
        }
    }

    /// Records creation evidence without gating independently prepared checks.
    pub(super) async fn check_write_basic(&mut self) -> Result<(), crate::ContractFailure> {
        let id = ContractCheckId::WriteBasic;
        if !self.capable(FileSystemCapability::Write) {
            let relative = self.context.relative_name("write-unavailable");
            let path = self.fixture.path(&relative).map_err(|error| {
                crate::ContractFailure::with_source("write/basic: path preparation failed", error).at(id)
            })?;
            let error = match self.fixture.file_system().open_writer(&path, Default::default()).await {
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
        let basic_bytes = bounded_payload(limit, b"async written", b'a');
        let relative = self.context.relative_name("write-create");
        let prepared = self
            .fixture
            .prepare_write(
                crate::internal::check_catalog::specification(id)
                    .write_scenario
                    .expect("write check has a preparation scenario"),
                &relative,
                &basic_bytes,
            )
            .await
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
        if case.bytes() != basic_bytes.as_slice()
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
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&path, case.options().clone())
            .await
            .map_err(|error| {
                crate::ContractFailure::with_owned_source("write/basic: write contract: writer open failed", error)
                    .at(id)
            })?;
        if let Err(error) = writer.write_fully_async(&basic_bytes).await {
            return Err(crate::ContractFailure::with_owned_source(
                "write/basic: write contract: writer rejected bytes",
                crate::ContractWriterFailure::new(error, writer),
            )
            .at(id));
        }
        let outcome = match writer.commit_async().await {
            Ok(outcome) => outcome,
            Err(error) => {
                return Err(crate::ContractFailure::with_owned_source(
                    "write/basic: write contract: writer commit failed",
                    crate::ContractWriterFailure::new(error, writer),
                )
                .at(id));
            }
        };
        if outcome
            .bytes_written()
            .is_some_and(|count| count != basic_bytes.len() as u64)
        {
            return Err(crate::ContractFailure::message_only("write/basic: published byte count mismatch").at(id));
        }
        let observed = self.fixture.read_file(&path).await.map_err(|error| {
            crate::ContractFailure::with_source("write/basic: fixture observation failed", error).at(id)
        })?;
        match observed {
            FixtureSupport::Supported(bytes) if bytes == basic_bytes => {}
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

/// Selects a write payload that fits the declared provider limit.
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
