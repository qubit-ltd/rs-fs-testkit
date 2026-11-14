// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![cfg(feature = "async")]

use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

pub use ::qubit_fs_testkit;
use qubit_fs::path::Path;
use qubit_fs_testkit::AsyncCopyCancellationStage;
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::CopyCancellationProbe;
use qubit_fs_testkit::FixtureFuture;
use qubit_fs_testkit::FixtureSupport;

mod common;

use self::common::AsyncMemoryFault;
use self::common::AsyncMemoryFixture;
use self::common::async_memory_file_system::run_controlled;

#[derive(Clone, Copy)]
enum RecoverySnapshotFailure {
    Existence,
    Bytes,
}

struct AsyncCopyCancellationFixture {
    inner: AsyncMemoryFixture,
    disarm_before_execution: bool,
    recovery_snapshot_failure: Option<RecoverySnapshotFailure>,
    target: Mutex<Option<Path>>,
    target_exists_calls: AtomicUsize,
}

impl AsyncFileSystemFixture for AsyncCopyCancellationFixture {
    fn file_system(&self) -> &qubit_fs::AsyncFileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn exists_out_of_band<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<bool>> {
        let is_target = self.target.lock().expect("target lock").as_ref() == Some(path);
        if is_target {
            let call = self.target_exists_calls.fetch_add(1, Ordering::Relaxed);
            if call != 0 && matches!(self.recovery_snapshot_failure, Some(RecoverySnapshotFailure::Existence)) {
                return Box::pin(async { Ok(FixtureSupport::Unsupported) });
            }
        }
        self.inner.exists_out_of_band(path)
    }

    fn read_file<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<Vec<u8>>> {
        let is_target = self.target.lock().expect("target lock").as_ref() == Some(path);
        if is_target && matches!(self.recovery_snapshot_failure, Some(RecoverySnapshotFailure::Bytes)) {
            Box::pin(async { Ok(FixtureSupport::Unsupported) })
        } else {
            self.inner.read_file(path)
        }
    }

    fn teardown(&self) -> FixtureFuture<'_, ()> {
        self.inner.teardown()
    }

    fn prepare_copy_cancellation<'a>(
        &'a self,
        stage: AsyncCopyCancellationStage,
        relative: &'a str,
    ) -> FixtureFuture<'a, FixtureSupport<Box<dyn CopyCancellationProbe>>> {
        Box::pin(async move {
            let prepared = self.inner.prepare_copy_cancellation(stage, relative).await?;
            if let FixtureSupport::Supported(probe) = &prepared {
                *self.target.lock().expect("target lock") = Some(probe.case().target().clone());
                self.target_exists_calls.store(0, Ordering::Relaxed);
                if self.disarm_before_execution {
                    probe.disarm()?;
                }
            }
            Ok(prepared)
        })
    }
}

/// Every asynchronous cancellation stage remains a distinct domain value.
#[test]
fn test_async_copy_cancellation_stages_are_distinct() {
    let stages = [
        AsyncCopyCancellationStage::NativeAttempt,
        AsyncCopyCancellationStage::Reader,
        AsyncCopyCancellationStage::Writer,
        AsyncCopyCancellationStage::Commit,
    ];

    for (index, stage) in stages.iter().enumerate() {
        assert_eq!(
            stages.iter().filter(|candidate| *candidate == stage).count(),
            1,
            "stage at index {index} must be unique",
        );
    }
}

#[test]
fn test_async_copy_cancellation_reports_probe_disarmed_before_acknowledgement() {
    let fixture = AsyncCopyCancellationFixture {
        inner: AsyncMemoryFixture::new(),
        disarm_before_execution: true,
        recovery_snapshot_failure: None,
        target: Mutex::new(None),
        target_exists_calls: AtomicUsize::new(0),
    };
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::AsyncCopyCancelNativeAttempt).await;
        assert!(!run.requirements_satisfied(), "a disarmed probe must fail");
        assert!(
            run.failures()
                .iter()
                .any(|failure| { failure.check() == Some(ContractCheckId::AsyncCopyCancelNativeAttempt) })
        );
        assert!(matches!(
            run.report()
                .checks()
                .iter()
                .find(|check| check.id() == ContractCheckId::AsyncCopyCancelNativeAttempt)
                .expect("selected copy cancellation outcome")
                .outcome(),
            ContractCheckOutcome::Failed { .. }
        ));
        assert!(
            run.cleanup().completed(),
            "probe failure must still disarm its resources"
        );
    });
    assert!(fixture.inner.is_empty(), "probe-failure fixture must be cleaned");
}

#[test]
fn test_async_copy_cancellation_reports_unavailable_recovery_snapshots() {
    for (failure_mode, expected_reason) in [
        (RecoverySnapshotFailure::Existence, "no independent existence evidence"),
        (RecoverySnapshotFailure::Bytes, "no independent byte evidence"),
    ] {
        let fixture = AsyncCopyCancellationFixture {
            inner: AsyncMemoryFixture::with_fault(AsyncMemoryFault::CopyAbortPublishes),
            disarm_before_execution: false,
            recovery_snapshot_failure: Some(failure_mode),
            target: Mutex::new(None),
            target_exists_calls: AtomicUsize::new(0),
        };
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::AsyncCopyCancelCommit).await;
            assert!(
                !run.requirements_satisfied(),
                "missing recovery evidence must fail the check"
            );
            assert!(run.failures().iter().any(|failure| {
                failure.check() == Some(ContractCheckId::AsyncCopyCancelCommit)
                    && failure.message().contains(expected_reason)
            }));
            assert!(matches!(
                run.report()
                    .checks()
                    .iter()
                    .find(|check| check.id() == ContractCheckId::AsyncCopyCancelCommit)
                    .expect("selected copy cancellation outcome")
                    .outcome(),
                ContractCheckOutcome::Failed { .. }
            ));
            assert!(
                run.cleanup().completed(),
                "failed recovery snapshot must still disarm its probe"
            );
        });
        assert!(fixture.inner.is_empty(), "recovery-snapshot fixture must be cleaned");
    }
}
