// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Independent asynchronous non-empty native copy evidence.

use qubit_fs::copy::CopyMethod;
use qubit_fs::copy::CopyMode;
use qubit_fs::metadata::FileSystemCapability;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl AsyncFileSystemContractSuite<'_> {
    /// Verifies that an independent non-empty file is copied natively.
    pub(super) async fn check_native_copy_nonempty(&mut self) -> Result<(), ContractFailure> {
        let id = ContractCheckId::CopyNativeNonEmpty;
        self.context.begin_check(id);
        if !self.capable(FileSystemCapability::Copy) {
            self.context.record_check(
                id,
                Some(FileSystemCapability::Copy),
                ContractCheckOutcome::NotApplicable {
                    reason: "Copy capability is unavailable".to_owned(),
                },
            );
            return Ok(());
        }
        if self.fixture.copy_fallback_only() {
            self.context.record_check(
                id,
                Some(FileSystemCapability::Copy),
                ContractCheckOutcome::NotApplicable {
                    reason: "fixture only validates streaming fallback".to_owned(),
                },
            );
            return Ok(());
        }
        let case = match self
            .fixture
            .copy_fast_path_case(CopyMethod::Native)
            .await
            .map_err(|error| ContractFailure::with_source("native copy case preparation failed", error).at(id))?
        {
            FixtureSupport::Supported(case) => case,
            FixtureSupport::Unsupported => {
                self.context.record_check(
                    id,
                    Some(FileSystemCapability::Copy),
                    ContractCheckOutcome::SkippedOptional {
                        reason: "fixture has no independent non-empty native copy scenario".to_owned(),
                    },
                );
                return Ok(());
            }
        };
        let (source, target, options) = case.into_parts();
        self.context.record_created(source.clone());
        self.context.record_created(target.clone());
        verify_condition(source != target, id, "native copy source and target alias")?;
        verify_condition(
            options.mode() == CopyMode::File,
            id,
            "native copy case is not a single-file request",
        )?;
        let target_before = self
            .fixture
            .exists_out_of_band(&target)
            .await
            .map_err(|error| ContractFailure::with_source("native copy target observation failed", error).at(id))?;
        match target_before {
            FixtureSupport::Supported(false) => {}
            FixtureSupport::Supported(true) => {
                return Err(ContractFailure::message_only("native copy target already exists").at(id));
            }
            FixtureSupport::Unsupported => {
                self.context.record_check(
                    id,
                    Some(FileSystemCapability::Copy),
                    ContractCheckOutcome::Unverified {
                        reason: "fixture.exists_out_of_band cannot confirm native copy target absence".to_owned(),
                    },
                );
                return Ok(());
            }
        }
        let source_bytes = match self
            .fixture
            .read_file(&source)
            .await
            .map_err(|error| ContractFailure::with_source("native copy source observation failed", error).at(id))?
        {
            FixtureSupport::Supported(bytes) => bytes,
            FixtureSupport::Unsupported => {
                self.context.record_check(
                    id,
                    Some(FileSystemCapability::Copy),
                    ContractCheckOutcome::Unverified {
                        reason: "fixture.read_file cannot independently observe native copy source".to_owned(),
                    },
                );
                return Ok(());
            }
        };
        verify_condition(
            !source_bytes.is_empty()
                && source_bytes.len() <= crate::internal::limit_probe_plan::MAX_PROBE_BYTES as usize,
            id,
            "native copy source must contain 1..=64 KiB",
        )?;
        let outcome = crate::internal::execute_copy::execute_copy(
            self.fixture.file_system(),
            source.clone(),
            target.clone(),
            options,
        )
        .await
        .map_err(|error| ContractFailure::with_owned_source("native copy execution failed", error).at(id))?;
        verify_condition(
            outcome.method() == CopyMethod::Native && !outcome.used_fallback(),
            id,
            "copy did not complete with the Native method",
        )?;
        verify_condition(
            outcome.stats().bytes == source_bytes.len() as u64 && outcome.stats().files + outcome.stats().objects == 1,
            id,
            "native copy statistics differ",
        )?;
        for (path, expected) in [(&source, &source_bytes), (&target, &source_bytes)] {
            match self
                .fixture
                .read_file(path)
                .await
                .map_err(|error| ContractFailure::with_source("native copy final observation failed", error).at(id))?
            {
                FixtureSupport::Supported(actual) => verify_condition(
                    &actual == expected,
                    id,
                    "native copy changed source or published incorrect target bytes",
                )?,
                FixtureSupport::Unsupported => {
                    self.context.record_check(
                        id,
                        Some(FileSystemCapability::Copy),
                        ContractCheckOutcome::Unverified {
                            reason: "fixture.read_file cannot independently observe native copy result".to_owned(),
                        },
                    );
                    return Ok(());
                }
            }
        }
        self.context
            .record_check(id, Some(FileSystemCapability::Copy), ContractCheckOutcome::Passed);
        Ok(())
    }
}
