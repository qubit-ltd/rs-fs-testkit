// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

pub use ::qubit_fs_testkit;

mod common;

use qubit_fs::FileSystem;
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs::path::Path;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteOptions;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContractSuite;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixturePreparation;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;
use qubit_fs_testkit::WriteFixtureCase;

use self::common::MemoryFixture;
use self::common::shared_model::FixtureHook;

fn limits() -> FileSystemLimits {
    FileSystemLimits::unknown().with_max_write_bytes(FileSystemLimit::Maximum(4))
}

#[derive(Clone, Copy)]
enum ProbeMode {
    ExistingCreateNew,
    ExistingReplace,
    ExistingReplaceContentChanges,
    ExistingReadFailsBefore,
    ExistingReadFailsAfter,
    ReportsCreatedAfterRejection,
    ReportsUnsupportedAfterRejection,
    ExistingReadUnsupportedBefore,
    ExistingReadUnsupportedAfter,
}

struct ProbeFixture {
    inner: MemoryFixture,
    mode: ProbeMode,
    exists_calls: std::cell::Cell<usize>,
    read_calls: std::cell::Cell<usize>,
}

impl ProbeFixture {
    fn new(mode: ProbeMode) -> Self {
        Self {
            inner: MemoryFixture::with_fault_and_limits(common::MemoryFault::None, limits()),
            mode,
            exists_calls: std::cell::Cell::new(0),
            read_calls: std::cell::Cell::new(0),
        }
    }
}

impl FileSystemFixture for ProbeFixture {
    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn prepare_write(
        &self,
        scenario: qubit_fs_testkit::WriteScenario,
        relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<FixturePreparation<WriteFixtureCase>> {
        if bytes.len() > 4 {
            match self.mode {
                ProbeMode::ExistingCreateNew
                | ProbeMode::ExistingReplace
                | ProbeMode::ExistingReplaceContentChanges
                | ProbeMode::ExistingReadFailsBefore
                | ProbeMode::ExistingReadFailsAfter
                | ProbeMode::ExistingReadUnsupportedBefore
                | ProbeMode::ExistingReadUnsupportedAfter => {
                    let FixtureSupport::Supported(path) = self.inner.seed_file(relative, b"prior target bytes")? else {
                        return Ok(FixturePreparation::Unavailable {
                            reason: "seed unsupported".into(),
                        });
                    };
                    let disposition = match self.mode {
                        ProbeMode::ExistingCreateNew => WriteDisposition::CreateNew,
                        ProbeMode::ReportsCreatedAfterRejection => unreachable!(),
                        ProbeMode::ReportsUnsupportedAfterRejection => unreachable!(),
                        ProbeMode::ExistingReplace
                        | ProbeMode::ExistingReplaceContentChanges
                        | ProbeMode::ExistingReadFailsBefore
                        | ProbeMode::ExistingReadFailsAfter
                        | ProbeMode::ExistingReadUnsupportedBefore
                        | ProbeMode::ExistingReadUnsupportedAfter => WriteDisposition::CreateOrReplace,
                    };
                    return Ok(FixturePreparation::Ready(WriteFixtureCase::new(
                        path,
                        bytes.to_vec(),
                        WriteOptions::default().with_disposition(disposition),
                    )));
                }
                ProbeMode::ReportsCreatedAfterRejection | ProbeMode::ReportsUnsupportedAfterRejection => {}
            }
        }
        self.inner.prepare_write(scenario, relative, bytes)
    }

    fn exists_out_of_band(&self, path: &Path) -> FixtureResult<FixtureSupport<bool>> {
        let call = self.exists_calls.get() + 1;
        self.exists_calls.set(call);
        if matches!(self.mode, ProbeMode::ReportsCreatedAfterRejection) && call == 2 {
            return Ok(FixtureSupport::Supported(true));
        }
        if matches!(self.mode, ProbeMode::ReportsUnsupportedAfterRejection) && call == 2 {
            return Ok(FixtureSupport::Unsupported);
        }
        self.inner.exists_out_of_band(path)
    }

    fn read_file(&self, path: &Path) -> FixtureResult<FixtureSupport<Vec<u8>>> {
        let call = self.read_calls.get() + 1;
        self.read_calls.set(call);
        if matches!(self.mode, ProbeMode::ExistingReadFailsBefore) && call == 2
            || matches!(self.mode, ProbeMode::ExistingReadFailsAfter) && call == 3
        {
            return Err(FixtureError::new("injected WriteLimit content observation failure"));
        }
        if matches!(self.mode, ProbeMode::ExistingReadUnsupportedBefore) && call == 2
            || matches!(self.mode, ProbeMode::ExistingReadUnsupportedAfter) && call == 3
        {
            return Ok(FixtureSupport::Unsupported);
        }
        if matches!(self.mode, ProbeMode::ExistingReplaceContentChanges) && call == 3 {
            return Ok(FixtureSupport::Supported(b"changed after rejection".to_vec()));
        }
        self.inner.read_file(path)
    }

    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }
}

#[test]
fn test_write_limit_requires_independent_target_baseline() {
    let fixture = MemoryFixture::with_fixture_hook_error_and_limits(FixtureHook::Exists, 1, limits());
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteLimit);
    assert!(!run.requirements_satisfied());
    assert_eq!(run.failures()[0].check(), Some(ContractCheckId::WriteLimit));
}

#[test]
fn test_write_limit_reports_unsupported_baseline_as_unverified() {
    let fixture = MemoryFixture::with_unsupported_fixture_hook_and_limits(FixtureHook::Exists, 1, limits());
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteLimit);
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::Unverified { .. }
    ));
}

#[test]
fn test_write_limit_requires_post_cleanup_target_observation() {
    let fixture = MemoryFixture::with_fixture_hook_error_and_limits(FixtureHook::Exists, 2, limits());
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteLimit);
    assert!(!run.requirements_satisfied());
    assert_eq!(run.failures()[0].check(), Some(ContractCheckId::WriteLimit));
}

#[test]
fn test_write_limit_without_finite_probe_is_not_a_false_failure() {
    let fixture = MemoryFixture::with_all_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteLimit);
    assert!(run.requirements_satisfied());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::SkippedOptional { .. }
    ));
}

#[test]
fn test_write_limit_absent_target_remains_absent() {
    let fixture = MemoryFixture::with_fault_and_limits(common::MemoryFault::None, limits());
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteLimit);
    assert!(run.requirements_satisfied(), "{:?}", run.failures());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::Passed
    ));
}

#[test]
fn test_write_limit_create_new_rejects_preexisting_target_as_bad_premise() {
    let fixture = ProbeFixture::new(ProbeMode::ExistingCreateNew);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteLimit);
    assert!(!run.requirements_satisfied());
    assert_eq!(run.failures()[0].check(), Some(ContractCheckId::WriteLimit));
}

#[test]
fn test_write_limit_preserves_existing_replace_target() {
    let fixture = ProbeFixture::new(ProbeMode::ExistingReplace);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteLimit);
    assert!(run.requirements_satisfied(), "{:?}", run.failures());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::Passed
    ));
}

#[test]
fn test_write_limit_detects_existing_target_content_change() {
    let fixture = ProbeFixture::new(ProbeMode::ExistingReplaceContentChanges);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteLimit);
    assert!(!run.requirements_satisfied());
    assert_eq!(run.failures()[0].check(), Some(ContractCheckId::WriteLimit));
}

#[test]
fn test_write_limit_preserves_baseline_content_observation_errors() {
    for mode in [ProbeMode::ExistingReadFailsBefore, ProbeMode::ExistingReadFailsAfter] {
        let fixture = ProbeFixture::new(mode);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::WriteLimit);
        assert!(!run.requirements_satisfied());
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::WriteLimit));
    }
}

#[test]
fn test_write_limit_detects_target_appearing_after_rejection() {
    let fixture = ProbeFixture::new(ProbeMode::ReportsCreatedAfterRejection);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteLimit);
    assert!(!run.requirements_satisfied());
    assert_eq!(run.failures()[0].check(), Some(ContractCheckId::WriteLimit));
}

#[test]
fn test_write_limit_reports_unsupported_post_rejection_target_as_unverified() {
    let fixture = MemoryFixture::with_unsupported_fixture_hook_and_limits(FixtureHook::Exists, 2, limits());
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteLimit);
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::Unverified { .. }
    ));
}

#[test]
fn test_write_limit_probe_fixture_reports_unsupported_post_target_as_unverified() {
    let fixture = ProbeFixture::new(ProbeMode::ReportsUnsupportedAfterRejection);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteLimit);
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::Unverified { .. }
    ));
}

#[test]
fn test_write_limit_reports_unsupported_existing_contents_as_unverified() {
    for mode in [
        ProbeMode::ExistingReadUnsupportedBefore,
        ProbeMode::ExistingReadUnsupportedAfter,
    ] {
        let fixture = ProbeFixture::new(mode);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::WriteLimit);
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { .. }
        ));
    }
}

#[cfg(feature = "async")]
#[test]
fn test_async_write_limit_requires_independent_target_baseline() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::with_fixture_hook_error_and_limits(FixtureHook::Exists, 1, limits());
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::WriteLimit).await;
        assert!(!run.requirements_satisfied());
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::WriteLimit));
    });
}

#[cfg(feature = "async")]
#[test]
fn test_async_write_limit_reports_unsupported_baseline_as_unverified() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::with_unsupported_fixture_hook_and_limits(FixtureHook::Exists, 1, limits());
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::WriteLimit).await;
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { .. }
        ));
    });
}

#[cfg(feature = "async")]
#[test]
fn test_async_write_limit_requires_post_cleanup_target_observation() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::with_fixture_hook_error_and_limits(FixtureHook::Exists, 2, limits());
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::WriteLimit).await;
        assert!(!run.requirements_satisfied());
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::WriteLimit));
    });
}

#[cfg(feature = "async")]
mod async_write_limit_target_observation {
    use std::future::Future;
    use std::future::poll_fn;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::task::Context;
    use std::task::Poll;
    use std::task::Waker;

    use qubit_fs::AsyncFileSystem;
    use qubit_fs::path::Path;
    use qubit_fs::write::WriteDisposition;
    use qubit_fs::write::WriteOptions;
    use qubit_fs_testkit::AsyncFileSystemContractSuite;
    use qubit_fs_testkit::AsyncFileSystemFixture;
    use qubit_fs_testkit::FixtureFuture;

    use super::ContractCheckId;
    use super::ContractCheckOutcome;
    use super::FixtureError;
    use super::FixturePreparation;
    use super::FixtureResult;
    use super::FixtureSupport;
    use super::ProbeMode;
    use super::WriteFixtureCase;
    use super::limits;
    use crate::common::AsyncMemoryFixture;
    use crate::common::async_memory_file_system::run_controlled;

    struct AsyncProbeFixture {
        inner: AsyncMemoryFixture,
        mode: ProbeMode,
        exists_calls: AtomicUsize,
        read_calls: AtomicUsize,
        pause_exists_call: Option<usize>,
        pause_exists_gate: Option<Arc<AtomicBool>>,
    }

    impl AsyncProbeFixture {
        fn new(mode: ProbeMode) -> Self {
            Self {
                inner: AsyncMemoryFixture::with_fault_and_limits(crate::common::AsyncMemoryFault::None, limits()),
                mode,
                exists_calls: AtomicUsize::new(0),
                read_calls: AtomicUsize::new(0),
                pause_exists_call: None,
                pause_exists_gate: None,
            }
        }

        fn pause_exists_observation(mut self, call: usize, gate: Arc<AtomicBool>) -> Self {
            self.pause_exists_call = Some(call);
            self.pause_exists_gate = Some(gate);
            self
        }
    }

    impl AsyncFileSystemFixture for AsyncProbeFixture {
        fn file_system(&self) -> &AsyncFileSystem {
            self.inner.file_system()
        }

        fn path(&self, relative: &str) -> FixtureResult<Path> {
            self.inner.path(relative)
        }

        fn teardown(&self) -> FixtureFuture<'_, ()> {
            self.inner.teardown()
        }

        fn prepare_write<'a>(
            &'a self,
            scenario: qubit_fs_testkit::WriteScenario,
            relative: &'a str,
            bytes: &'a [u8],
        ) -> FixtureFuture<'a, FixturePreparation<WriteFixtureCase>> {
            Box::pin(async move {
                if bytes.len() <= 4 || matches!(self.mode, ProbeMode::ReportsCreatedAfterRejection) {
                    return self.inner.prepare_write(scenario, relative, bytes).await;
                }
                let FixtureSupport::Supported(path) = self.inner.seed_file(relative, b"prior target bytes").await?
                else {
                    return Ok(FixturePreparation::Unavailable {
                        reason: "seed unsupported".into(),
                    });
                };
                let disposition = match self.mode {
                    ProbeMode::ExistingCreateNew => WriteDisposition::CreateNew,
                    ProbeMode::ExistingReplace
                    | ProbeMode::ExistingReplaceContentChanges
                    | ProbeMode::ExistingReadFailsBefore
                    | ProbeMode::ExistingReadFailsAfter
                    | ProbeMode::ExistingReadUnsupportedBefore
                    | ProbeMode::ExistingReadUnsupportedAfter
                    | ProbeMode::ReportsCreatedAfterRejection
                    | ProbeMode::ReportsUnsupportedAfterRejection => WriteDisposition::CreateOrReplace,
                };
                Ok(FixturePreparation::Ready(WriteFixtureCase::new(
                    path,
                    bytes.to_vec(),
                    WriteOptions::default().with_disposition(disposition),
                )))
            })
        }

        fn seed_file<'a>(&'a self, relative: &'a str, bytes: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<Path>> {
            self.inner.seed_file(relative, bytes)
        }

        fn exists_out_of_band<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<bool>> {
            Box::pin(async move {
                let call = self.exists_calls.fetch_add(1, Ordering::Relaxed) + 1;
                if self.pause_exists_call == Some(call)
                    && let Some(gate) = &self.pause_exists_gate
                {
                    let gate = Arc::clone(gate);
                    poll_fn(move |_| {
                        if gate.load(Ordering::Relaxed) {
                            Poll::Pending
                        } else {
                            Poll::Ready(())
                        }
                    })
                    .await;
                }
                if call == 2 {
                    match self.mode {
                        ProbeMode::ReportsCreatedAfterRejection => return Ok(FixtureSupport::Supported(true)),
                        ProbeMode::ReportsUnsupportedAfterRejection => return Ok(FixtureSupport::Unsupported),
                        _ => {}
                    }
                }
                self.inner.exists_out_of_band(path).await
            })
        }

        fn read_file<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<Vec<u8>>> {
            Box::pin(async move {
                let call = self.read_calls.fetch_add(1, Ordering::Relaxed) + 1;
                if matches!(self.mode, ProbeMode::ExistingReadFailsBefore) && call == 2
                    || matches!(self.mode, ProbeMode::ExistingReadFailsAfter) && call == 3
                {
                    return Err(FixtureError::new(
                        "injected async WriteLimit content observation failure",
                    ));
                }
                if matches!(self.mode, ProbeMode::ExistingReadUnsupportedBefore) && call == 2
                    || matches!(self.mode, ProbeMode::ExistingReadUnsupportedAfter) && call == 3
                {
                    return Ok(FixtureSupport::Unsupported);
                }
                if matches!(self.mode, ProbeMode::ExistingReplaceContentChanges) && call == 3 {
                    return Ok(FixtureSupport::Supported(b"changed after rejection".to_vec()));
                }
                self.inner.read_file(path).await
            })
        }
    }

    fn assert_failed(run: &qubit_fs_testkit::ContractRun) {
        assert!(!run.requirements_satisfied());
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::WriteLimit));
    }

    #[test]
    fn test_existing_create_new_is_rejected_as_invalid_premise() {
        let fixture = AsyncProbeFixture::new(ProbeMode::ExistingCreateNew);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::WriteLimit).await;
            assert_failed(run);
        });
    }

    #[test]
    fn test_existing_replace_target_remains_unchanged_after_rejection() {
        let fixture = AsyncProbeFixture::new(ProbeMode::ExistingReplace);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::WriteLimit).await;
            assert!(run.requirements_satisfied(), "{:?}", run.failures());
            assert!(matches!(
                run.report().checks()[0].outcome(),
                ContractCheckOutcome::Passed
            ));
        });
    }

    #[test]
    fn test_existing_replace_content_change_is_rejected() {
        let fixture = AsyncProbeFixture::new(ProbeMode::ExistingReplaceContentChanges);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::WriteLimit).await;
            assert_failed(run);
        });
    }

    #[test]
    fn test_target_appearing_after_rejection_is_rejected() {
        let fixture = AsyncProbeFixture::new(ProbeMode::ReportsCreatedAfterRejection);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::WriteLimit).await;
            assert_failed(run);
        });
    }

    #[test]
    fn test_post_rejection_exists_unsupported_is_unverified() {
        let fixture = AsyncProbeFixture::new(ProbeMode::ReportsUnsupportedAfterRejection);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::WriteLimit).await;
            assert!(matches!(
                run.report().checks()[0].outcome(),
                ContractCheckOutcome::Unverified { .. }
            ));
        });
    }

    #[test]
    fn test_baseline_and_post_rejection_content_errors_fail() {
        for mode in [ProbeMode::ExistingReadFailsBefore, ProbeMode::ExistingReadFailsAfter] {
            let fixture = AsyncProbeFixture::new(mode);
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(ContractCheckId::WriteLimit).await;
                assert_failed(run);
            });
        }
    }

    #[test]
    fn test_baseline_and_post_rejection_content_unsupported_is_unverified() {
        for mode in [
            ProbeMode::ExistingReadUnsupportedBefore,
            ProbeMode::ExistingReadUnsupportedAfter,
        ] {
            let fixture = AsyncProbeFixture::new(mode);
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(ContractCheckId::WriteLimit).await;
                assert!(matches!(
                    run.report().checks()[0].outcome(),
                    ContractCheckOutcome::Unverified { .. }
                ));
            });
        }
    }

    #[test]
    fn test_cancellation_during_baseline_observation_keeps_cleanup_ledger() {
        let gate = Arc::new(AtomicBool::new(true));
        let fixture = AsyncProbeFixture::new(ProbeMode::ExistingReplace).pause_exists_observation(1, Arc::clone(&gate));
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let mut check = Box::pin(suite.run_check(ContractCheckId::WriteLimit));
        assert!(matches!(
            check.as_mut().poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
        drop(check);

        gate.store(false, Ordering::Relaxed);
        run_controlled(async {
            suite.finish().await;
            assert!(
                fixture.inner.is_empty(),
                "cancellation must leave the target registered for cleanup"
            );
        });
    }

    #[test]
    fn test_cancellation_during_post_cleanup_observation_keeps_cleanup_ledger() {
        let gate = Arc::new(AtomicBool::new(true));
        let fixture = AsyncProbeFixture::new(ProbeMode::ExistingReplace).pause_exists_observation(2, Arc::clone(&gate));
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let mut check = Box::pin(suite.run_check(ContractCheckId::WriteLimit));
        assert!(matches!(
            check.as_mut().poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
        drop(check);

        gate.store(false, Ordering::Relaxed);
        run_controlled(async {
            suite.finish().await;
            assert!(
                fixture.inner.is_empty(),
                "cancellation must leave the target registered for cleanup"
            );
        });
    }
}
