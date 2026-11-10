// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Independently checks random-access sessions in the synchronous driver.

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::read::ReadOptions;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FileSystemContractSuite;
use crate::FixtureSupport;

const SEED: &[u8] = b"abcdef";

impl FileSystemContractSuite<'_> {
    /// Executes one independently prepared random-access check.
    ///
    /// # Parameters
    ///
    /// * `id` — the stable identity of the contract check being executed or
    ///   recorded.
    pub(super) fn execute_random_access(
        &mut self,
        id: ContractCheckId,
    ) -> Result<ContractCheckOutcome, ContractFailure> {
        match self.fixture.random_access_operation() {
            FixtureSupport::Unsupported => {
                return Ok(ContractCheckOutcome::SkippedOptional {
                    reason: "fixture has no independent random-access operation declaration".to_owned(),
                });
            }
            FixtureSupport::Supported(false) => {
                let path = self
                    .fixture
                    .path(&self.context.relative_name("random-access"))
                    .map_err(|error| {
                        ContractFailure::with_source("random-access path preparation failed", error).at(id)
                    })?;
                match self
                    .fixture
                    .file_system()
                    .open_random_access_reader(&path, ReadOptions::default())
                {
                    Err(error) if error.kind() == FsErrorKind::UnsupportedOperation => {
                        return Ok(ContractCheckOutcome::RejectedAsExpected);
                    }
                    Err(error) => {
                        return Err(ContractFailure::with_source(
                            "undeclared random-access operation returned an unexpected error",
                            error,
                        )
                        .at(id));
                    }
                    Ok(_) => {
                        return Err(ContractFailure::message_only(
                            "provider opened a random-access reader after declaring the operation unsupported",
                        )
                        .at(id));
                    }
                }
            }
            FixtureSupport::Supported(true) => {}
        }

        let range_limit = self.context.properties().limits().max_read_range_bytes();
        let relative = self.context.relative_name("random-access");
        let path = match self
            .fixture
            .seed_file(&relative, SEED)
            .map_err(|error| ContractFailure::with_source("random-access seed preparation failed", error).at(id))?
        {
            FixtureSupport::Supported(path) => path,
            FixtureSupport::Unsupported => {
                return Ok(ContractCheckOutcome::Unverified {
                    reason: "provider declares random access but fixture could not prepare the seed".to_owned(),
                });
            }
        };
        self.context.record_created(path.clone());
        let mut reader = self
            .fixture
            .file_system()
            .open_random_access_reader(&path, ReadOptions::default())
            .map_err(|error| ContractFailure::with_source("declared random-access open failed", error).at(id))?;
        if reader.len() != SEED.len() as u64 {
            return Err(ContractFailure::message_only("random-access open-time length did not match the seed").at(id));
        }

        if id == ContractCheckId::RandomAccessBasic {
            verify_sample(&mut reader, 4, b"ef", range_limit, id)?;
            verify_sample(&mut reader, 0, b"ab", range_limit, id)?;
            verify_sample(&mut reader, 4, b"ef", range_limit, id)?;
        } else if id == ContractCheckId::RandomAccessBounds {
            let mut empty = [];
            if reader
                .read_at(u64::MAX, &mut empty)
                .map_err(|error| ContractFailure::with_source("empty random-access read failed", error).at(id))?
                != 0
            {
                return Err(ContractFailure::message_only("empty random-access read returned bytes").at(id));
            }
            let mut sample = [0; 2];
            if reader
                .read_at(6, &mut sample)
                .map_err(|error| ContractFailure::with_source("EOF random-access read failed", error).at(id))?
                != 0
            {
                return Err(ContractFailure::message_only("random-access read past EOF returned bytes").at(id));
            }
            let mut clipped = [0; 2];
            let clipped_count = reader
                .read_at(5, &mut clipped)
                .map_err(|error| ContractFailure::with_source("open-time length clipping read failed", error).at(id))?;
            if clipped_count != 1 || clipped[0] != b'f' {
                return Err(ContractFailure::message_only(
                    "random-access request was not clipped to the open-time length",
                )
                .at(id));
            }
            let overflow = reader
                .read_at(u64::MAX, &mut sample)
                .expect_err("overflow must be rejected");
            if overflow.kind() != FsErrorKind::InvalidOptions {
                return Err(ContractFailure::message_only(
                    "overflowing random-access range did not return InvalidOptions",
                )
                .at(id));
            }
            if let FileSystemLimit::Maximum(maximum) = self.context.properties().limits().max_read_range_bytes() {
                let Some((accepted, rejected)) =
                    crate::internal::limit_probe_plan::finite_probe(FileSystemLimit::Maximum(maximum), 64)
                else {
                    return Ok(ContractCheckOutcome::SkippedOptional {
                        reason: "finite random-access range limit exceeds the 64-byte probe budget".to_owned(),
                    });
                };
                if rejected > SEED.len() as u64 {
                    return Ok(ContractCheckOutcome::SkippedOptional {
                        reason: "seed is too short to probe the declared random-access range limit".to_owned(),
                    });
                }
                if accepted > 0 {
                    verify_sample(&mut reader, 0, &SEED[..accepted as usize], range_limit, id)?;
                }
                let mut rejected_buffer = vec![0; rejected as usize];
                let error = reader
                    .read_at(0, &mut rejected_buffer)
                    .expect_err("range above finite random-access limit must fail");
                if error.kind() != FsErrorKind::ResourceLimitExceeded || error.operation() != FsOperation::Read {
                    return Err(ContractFailure::message_only(
                        "range above finite random-access limit returned the wrong error",
                    )
                    .at(id));
                }
            }
            verify_sample(&mut reader, 0, b"ab", range_limit, id)?;
        } else {
            let FixtureSupport::Supported(()) = self
                .fixture
                .replace_random_access_path(&path, b"WXYZ")
                .map_err(|error| ContractFailure::with_source("random-access path replacement failed", error).at(id))?
            else {
                return Ok(ContractCheckOutcome::SkippedOptional {
                    reason: "fixture cannot replace the random-access path independently".to_owned(),
                });
            };
            verify_sample(&mut reader, 0, b"ab", range_limit, id)?;
            let mut replacement = self
                .fixture
                .file_system()
                .open_random_access_reader(&path, ReadOptions::default())
                .map_err(|error| ContractFailure::with_source("replacement random-access open failed", error).at(id))?;
            verify_sample(&mut replacement, 0, b"WX", range_limit, id)?;
        }
        Ok(ContractCheckOutcome::Passed)
    }
}

/// Reads the requested random-access range and compares it with the expected
/// bytes.
///
/// # Parameters
///
/// * `reader` — the open random-access reader used to retrieve the sample.
/// * `offset` — the byte position from which the random-access sample is read.
/// * `expected` — the bytes or value required by the selected contract check.
/// * `range_limit` — the maximum byte count permitted for one read request.
/// * `id` — the stable identity of the contract check being executed or
///   recorded.
fn verify_sample(
    reader: &mut qubit_fs::read::RandomAccessReader,
    offset: u64,
    expected: &[u8],
    range_limit: FileSystemLimit,
    id: ContractCheckId,
) -> Result<(), ContractFailure> {
    let mut actual = vec![0; expected.len()];
    let maximum = range_limit
        .maximum()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(usize::MAX);
    let mut filled = 0;
    while filled < actual.len() {
        let request_len = (actual.len() - filled).min(maximum);
        if request_len == 0 {
            return Err(ContractFailure::message_only(
                "random-access range limit prevents a non-empty sample read",
            )
            .at(id));
        }
        let count = reader
            .read_at(offset + filled as u64, &mut actual[filled..filled + request_len])
            .map_err(|error| ContractFailure::with_source("random-access sample read failed", error).at(id))?;
        if count == 0 {
            break;
        }
        filled += count;
    }
    if actual[..filled] != *expected {
        return Err(ContractFailure::message_only(format!(
            "random-access sample mismatch at offset {offset}: read {filled} of {} bytes",
            expected.len()
        ))
        .at(id));
    }
    Ok(())
}
