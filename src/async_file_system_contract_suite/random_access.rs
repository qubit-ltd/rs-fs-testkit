// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Independently checks asynchronous random-access sessions.

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::pin::pin;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::read::ReadOptions;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixtureSupport;

const SEED: &[u8] = b"abcdef";

impl AsyncFileSystemContractSuite<'_> {
    /// Executes one independently prepared random-access check.
    ///
    /// Random-access fixture declarations are consulted only when the captured
    /// provider properties include `Read`; otherwise the check is not
    /// applicable.
    ///
    /// # Parameters
    ///
    /// * `id` — the stable identity of the contract check being executed or
    ///   recorded.
    pub(super) async fn execute_random_access(
        &mut self,
        id: ContractCheckId,
    ) -> Result<ContractCheckOutcome, ContractFailure> {
        if !self.capable(FileSystemCapability::Read) {
            return Ok(ContractCheckOutcome::NotApplicable {
                reason: "random-access checks require the Read capability".to_owned(),
            });
        }
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
                    .await
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
        if id == ContractCheckId::RandomAccessRangeLimit {
            let limit = self.context.properties().limits().max_read_range_bytes();
            let FileSystemLimit::Maximum(maximum) = limit else {
                return Ok(ContractCheckOutcome::NotApplicable {
                    reason: format!("random-access range limit is not finite: {limit:?}"),
                });
            };
            let Some((accepted, rejected)) =
                crate::internal::limit_probe_plan::finite_probe(FileSystemLimit::Maximum(maximum), 64)
            else {
                return Ok(ContractCheckOutcome::SkippedOptional {
                    reason: "finite random-access range limit exceeds the 64-byte probe budget".to_owned(),
                });
            };
            let expected = (0..rejected as usize)
                .map(|index| SEED[index % SEED.len()])
                .collect::<Vec<_>>();
            let relative = self.context.relative_name("random-access-limit");
            let path = match self.fixture.seed_file(&relative, &expected).await.map_err(|error| {
                ContractFailure::with_source("random-access limit seed preparation failed", error).at(id)
            })? {
                FixtureSupport::Supported(path) => path,
                FixtureSupport::Unsupported => {
                    return Ok(ContractCheckOutcome::Unverified {
                        reason: "fixture could not prepare a seed for the finite random-access limit".to_owned(),
                    });
                }
            };
            self.context.record_created(path.clone());
            let mut reader = self
                .fixture
                .file_system()
                .open_random_access_reader(&path, ReadOptions::default())
                .await
                .map_err(|error| ContractFailure::with_source("random-access limit seed open failed", error).at(id))?;
            if reader.len() != rejected {
                return Err(ContractFailure::message_only(
                    "random-access limit seed open-time length did not match the seed",
                )
                .at(id));
            }
            let mut empty = [];
            let empty_before = reader
                .read_at(0, &mut empty)
                .await
                .map_err(|error| ContractFailure::with_source("empty random-access range read failed", error).at(id))?;
            if empty_before != 0 {
                return Err(ContractFailure::message_only("empty random-access range read returned bytes").at(id));
            }
            if accepted > 0 {
                verify_sample(&mut reader, 0, &expected[..accepted as usize], limit, id).await?;
            }
            let mut rejected_buffer = vec![0; rejected as usize];
            let error = reader
                .read_at(0, &mut rejected_buffer)
                .await
                .expect_err("range above finite random-access limit must fail");
            if error.kind() != FsErrorKind::ResourceLimitExceeded || error.operation() != FsOperation::Read {
                return Err(ContractFailure::message_only(
                    "range above finite random-access limit returned the wrong error",
                )
                .at(id));
            }
            let empty_after = reader.read_at(0, &mut empty).await.map_err(|error| {
                ContractFailure::with_source("empty random-access read after range rejection failed", error).at(id)
            })?;
            if empty_after != 0 {
                return Err(ContractFailure::message_only(
                    "empty random-access read after range rejection returned bytes",
                )
                .at(id));
            }
            return Ok(ContractCheckOutcome::Passed);
        }
        let range_limit = self.context.properties().limits().max_read_range_bytes();
        let relative = self.context.relative_name("random-access");
        let path = match self
            .fixture
            .seed_file(&relative, SEED)
            .await
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
        if id == ContractCheckId::RandomAccessCancel {
            return self.execute_random_access_cancel(&path, range_limit, id).await;
        }
        let mut reader = self
            .fixture
            .file_system()
            .open_random_access_reader(&path, ReadOptions::default())
            .await
            .map_err(|error| ContractFailure::with_source("declared random-access open failed", error).at(id))?;
        if reader.len() != SEED.len() as u64 {
            return Err(ContractFailure::message_only("random-access open-time length did not match the seed").at(id));
        }
        if matches!(range_limit, FileSystemLimit::Maximum(0))
            && matches!(
                id,
                ContractCheckId::RandomAccessBasic | ContractCheckId::RandomAccessIdentity
            )
        {
            return Ok(ContractCheckOutcome::SkippedOptional {
                reason: "zero random-access range limit prevents non-empty content or identity evidence".to_owned(),
            });
        }
        match id {
            ContractCheckId::RandomAccessBasic => {
                verify_sample(&mut reader, 4, b"ef", range_limit, id).await?;
                verify_sample(&mut reader, 0, b"ab", range_limit, id).await?;
                verify_sample(&mut reader, 4, b"ef", range_limit, id).await?;
            }
            ContractCheckId::RandomAccessBounds => {
                if matches!(range_limit, FileSystemLimit::Maximum(0)) {
                    return Ok(ContractCheckOutcome::SkippedOptional {
                        reason: "zero random-access range limit prevents non-empty boundary probes".to_owned(),
                    });
                }
                let mut empty = [];
                if reader
                    .read_at(u64::MAX, &mut empty)
                    .await
                    .map_err(|error| ContractFailure::with_source("empty random-access read failed", error).at(id))?
                    != 0
                {
                    return Err(ContractFailure::message_only("empty random-access read returned bytes").at(id));
                }
                let mut sample = [0; 2];
                if reader
                    .read_at(6, &mut sample)
                    .await
                    .map_err(|error| ContractFailure::with_source("EOF random-access read failed", error).at(id))?
                    != 0
                {
                    return Err(ContractFailure::message_only("random-access read past EOF returned bytes").at(id));
                }
                let mut clipped = [0; 2];
                let clipped_count = reader.read_at(5, &mut clipped).await.map_err(|error| {
                    ContractFailure::with_source("open-time length clipping read failed", error).at(id)
                })?;
                if clipped_count != 1 || clipped[0] != b'f' {
                    return Err(ContractFailure::message_only(
                        "random-access request was not clipped to the open-time length",
                    )
                    .at(id));
                }
                let overflow = reader
                    .read_at(u64::MAX, &mut sample)
                    .await
                    .expect_err("overflow must be rejected");
                if overflow.kind() != FsErrorKind::InvalidOptions {
                    return Err(ContractFailure::message_only(
                        "overflowing random-access range did not return InvalidOptions",
                    )
                    .at(id));
                }

                verify_sample(&mut reader, 0, b"ab", range_limit, id).await?;
            }
            ContractCheckId::RandomAccessRangeLimit => {
                unreachable!("range limit returned before seed setup")
            }
            ContractCheckId::RandomAccessIdentity => {
                let FixtureSupport::Supported(()) = self
                    .fixture
                    .replace_random_access_path(&path, b"WXYZ")
                    .await
                    .map_err(|error| {
                        ContractFailure::with_source("random-access path replacement failed", error).at(id)
                    })?
                else {
                    return Ok(ContractCheckOutcome::SkippedOptional {
                        reason: "fixture cannot replace the random-access path independently".to_owned(),
                    });
                };
                verify_sample(&mut reader, 0, b"ab", range_limit, id).await?;
                let mut replacement = self
                    .fixture
                    .file_system()
                    .open_random_access_reader(&path, ReadOptions::default())
                    .await
                    .map_err(|error| {
                        ContractFailure::with_source("replacement random-access open failed", error).at(id)
                    })?;
                verify_sample(&mut replacement, 0, b"WX", range_limit, id).await?;
            }
            ContractCheckId::RandomAccessCancel => {
                unreachable!("cancellation returned before opening the ordinary reader")
            }
            _ => unreachable!("random-access dispatch selected a different check"),
        }
        Ok(ContractCheckOutcome::Passed)
    }

    /// Cancels an injected pending read and verifies the reader remains usable.
    ///
    /// # Parameters
    ///
    /// * `path` — the filesystem path whose state or operation result is being
    ///   checked.
    /// * `id` — the stable identity of the contract check being executed or
    ///   recorded.
    async fn execute_random_access_cancel(
        &mut self,
        path: &qubit_fs::path::Path,
        range_limit: FileSystemLimit,
        id: ContractCheckId,
    ) -> Result<ContractCheckOutcome, ContractFailure> {
        let probe_len = range_limit
            .maximum()
            .and_then(|maximum| usize::try_from(maximum).ok())
            .map_or(2, |maximum| maximum.min(2));
        if probe_len == 0 {
            let reader = self
                .fixture
                .file_system()
                .open_random_access_reader(path, ReadOptions::default())
                .await
                .map_err(|error| ContractFailure::with_source("declared random-access open failed", error).at(id))?;
            if reader.len() != SEED.len() as u64 {
                return Err(
                    ContractFailure::message_only("random-access open-time length did not match the seed").at(id),
                );
            }
            return Ok(ContractCheckOutcome::SkippedOptional {
                reason: "zero random-access range limit prevents a non-empty pending-read probe".to_owned(),
            });
        }
        let arm_result = catch_unwind(AssertUnwindSafe(|| self.fixture.arm_random_access_pending_read(path)));
        match arm_result {
            Ok(Ok(FixtureSupport::Supported(()))) => {}
            Ok(Ok(FixtureSupport::Unsupported)) => {
                return Ok(ContractCheckOutcome::SkippedOptional {
                    reason: "fixture cannot inject a pending random-access read".to_owned(),
                });
            }
            Ok(Err(error)) => {
                let fixture = self.fixture;
                let probe_path = path;
                let mut guard = crate::internal::probe_disarm_guard::ProbeDisarmGuard::new(
                    move || fixture.disarm_random_access_pending_read(probe_path),
                    &mut self.context.run.cleanup.failures,
                );
                let _ = guard.disarm();
                return Err(ContractFailure::with_source("pending random-access probe arm failed", error).at(id));
            }
            Err(payload) => {
                let fixture = self.fixture;
                let probe_path = path;
                let mut guard = crate::internal::probe_disarm_guard::ProbeDisarmGuard::new(
                    move || fixture.disarm_random_access_pending_read(probe_path),
                    &mut self.context.run.cleanup.failures,
                );
                let _ = guard.disarm();
                return Err(ContractFailure::panicked("pending random-access probe arm panicked", payload).at(id));
            }
        }
        let fixture = self.fixture;
        let probe_path = path;
        let mut probe_guard = crate::internal::probe_disarm_guard::ProbeDisarmGuard::new(
            move || fixture.disarm_random_access_pending_read(probe_path),
            &mut self.context.run.cleanup.failures,
        );
        let mut reader = fixture
            .file_system()
            .open_random_access_reader(path, ReadOptions::default())
            .await
            .map_err(|error| ContractFailure::with_source("declared random-access open failed", error).at(id))?;
        if reader.len() != SEED.len() as u64 {
            return Err(ContractFailure::message_only("random-access open-time length did not match the seed").at(id));
        }
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        {
            let mut output = vec![0; probe_len];
            let read = reader.read_at(0, &mut output);
            drop(read);
        }
        let observed_pending = {
            let mut output = vec![0; probe_len];
            let mut read = pin!(reader.read_at(0, &mut output));
            matches!(read.as_mut().poll(&mut context), Poll::Pending)
        };
        let reached_gate = if observed_pending {
            self.fixture.random_access_pending_read_reached(path).map_err(|error| {
                ContractFailure::with_source("pending random-access gate observation failed", error).at(id)
            })?
        } else {
            FixtureSupport::Unsupported
        };
        let disarmed = probe_guard.disarm();
        drop(probe_guard);
        if !disarmed {
            return Err(ContractFailure::message_only("pending random-access probe could not be disarmed").at(id));
        }
        let range_limit = self.context.properties().limits().max_read_range_bytes();
        verify_sample(&mut reader, 2, b"cd", range_limit, id).await?;
        if !observed_pending {
            return Ok(ContractCheckOutcome::Unverified {
                reason: "injected read did not return Pending when first polled".to_owned(),
            });
        }
        if !matches!(reached_gate, FixtureSupport::Supported(true)) {
            return Ok(ContractCheckOutcome::Unverified {
                reason: "fixture did not confirm that the pending read reached the injected gate".to_owned(),
            });
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
async fn verify_sample(
    reader: &mut qubit_fs::read::AsyncRandomAccessReader,
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
            return Err(
                ContractFailure::message_only("random-access range limit prevents a non-empty sample read").at(id),
            );
        }
        let count = reader
            .read_at(offset + filled as u64, &mut actual[filled..filled + request_len])
            .await
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
