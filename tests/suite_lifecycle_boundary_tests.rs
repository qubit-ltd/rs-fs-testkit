// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

#![cfg(feature = "async")]

mod common;

use std::future::Future;
use std::future::poll_fn;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

pub use ::qubit_fs_testkit;
use qubit_fs::AsyncFileSystem;
use qubit_fs::FileSystem;
use qubit_fs::path::Path;
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureFuture;
use qubit_fs_testkit::FixtureResult;

use self::common::AsyncMemoryFixture;
use self::common::MemoryFixture;
use self::common::async_memory_file_system::run_controlled;

struct FailTeardownOnce {
    inner: MemoryFixture,
    fail: AtomicBool,
}

impl FileSystemFixture for FailTeardownOnce {
    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn teardown(&self) -> FixtureResult<()> {
        if self.fail.swap(false, Ordering::Relaxed) {
            Err(FixtureError::new("injected one-time teardown failure"))
        } else {
            Ok(())
        }
    }
}

struct PendingTeardownOnce {
    inner: AsyncMemoryFixture,
    calls: AtomicUsize,
}

struct FailAsyncTeardownOnce {
    inner: AsyncMemoryFixture,
    fail: AtomicBool,
}

impl AsyncFileSystemFixture for FailAsyncTeardownOnce {
    fn file_system(&self) -> &AsyncFileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn teardown(&self) -> FixtureFuture<'_, ()> {
        if self.fail.swap(false, Ordering::Relaxed) {
            Box::pin(async { Err(FixtureError::new("injected async teardown failure")) })
        } else {
            Box::pin(async { Ok(()) })
        }
    }
}

impl AsyncFileSystemFixture for PendingTeardownOnce {
    fn file_system(&self) -> &AsyncFileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn teardown(&self) -> FixtureFuture<'_, ()> {
        if self.calls.fetch_add(1, Ordering::Relaxed) == 0 {
            Box::pin(poll_fn(|_| Poll::Pending))
        } else {
            Box::pin(async { Ok(()) })
        }
    }
}

#[test]
fn test_sync_finish_closes_seed_and_required_seed_before_fixture_hooks() {
    for required in [false, true] {
        let fixture = MemoryFixture::without_delete();
        let mut suite = FileSystemContractSuite::new(&fixture);
        suite.finish();
        let paths = fixture.path_call_count();
        let seeds = fixture.seed_file_call_count();

        let result = catch_unwind(AssertUnwindSafe(|| {
            if required {
                let _ = suite.required_seed("after-finish", b"data", "lifecycle");
            } else {
                let _ = suite.seed("after-finish", b"data");
            }
        }));

        assert!(result.is_err());
        let payload = result.expect_err("preparation after finish must panic");
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&'static str>().copied())
            .expect("lifecycle panic should use a string payload");
        assert!(message.contains("session cleanup already started"));
        assert_eq!(fixture.path_call_count(), paths);
        assert_eq!(fixture.seed_file_call_count(), seeds);
        assert!(fixture.is_empty(), "rejected preparation must not add a resource");
    }
}

#[test]
fn test_cleanup_failure_keeps_preparation_closed_and_finish_can_be_retried() {
    let fixture = FailTeardownOnce {
        inner: MemoryFixture::without_delete(),
        fail: AtomicBool::new(true),
    };
    let mut suite = FileSystemContractSuite::new(&fixture);
    let _ = suite.seed("before-finish", b"data");
    assert!(catch_unwind(AssertUnwindSafe(|| suite.finish())).is_err());
    let paths = fixture.inner.path_call_count();
    let seeds = fixture.inner.seed_file_call_count();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = suite.seed("after-failed-finish", b"data");
        }))
        .is_err()
    );
    assert_eq!(fixture.inner.path_call_count(), paths);
    assert_eq!(fixture.inner.seed_file_call_count(), seeds);
    suite.finish();
    suite.finish();
}

#[test]
fn test_sync_run_cleanup_closes_required_seed_and_valid_seed_then_run_still_works() {
    let fixture = MemoryFixture::without_delete();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let _ = suite.run_contract(FileSystemContract::ErrorContext);
    let paths = fixture.path_call_count();
    let seeds = fixture.seed_file_call_count();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = suite.required_seed("after-run", b"data", "lifecycle");
        }))
        .is_err()
    );
    assert_eq!(fixture.path_call_count(), paths);
    assert_eq!(fixture.seed_file_call_count(), seeds);

    let fixture = MemoryFixture::new();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let _ = suite.seed("before-run", b"data");
    assert!(
        suite
            .run_contract(FileSystemContract::ErrorContext)
            .requirements_satisfied()
    );
    assert!(!suite.run_contract(FileSystemContract::Properties).failures().is_empty());
}

#[test]
fn test_sync_finish_closes_all_run_entries_without_fixture_path_mapping() {
    for entry in 0..3 {
        let fixture = MemoryFixture::new();
        let mut suite = FileSystemContractSuite::new(&fixture);
        suite.finish();
        let paths = fixture.path_call_count();
        let run = match entry {
            0 => suite.run_check(ContractCheckId::ErrorContext),
            1 => suite.run_contract(FileSystemContract::Properties),
            _ => suite.run_all(),
        };
        assert!(!run.failures().is_empty());
        assert!(
            run.failures()[0]
                .to_string()
                .contains("session cleanup already started")
        );
        assert_eq!(fixture.path_call_count(), paths);
    }
}

#[test]
fn test_async_finish_closes_required_seed_and_run_entries_before_fixture_path_mapping() {
    run_controlled(async {
        let fixture = AsyncMemoryFixture::new();
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        suite.finish().await;
        let paths = fixture.path_call_count();
        let seeds = fixture.seed_file_call_count();
        let result = catch_unwind(AssertUnwindSafe(|| {
            run_controlled(suite.required_seed("after-finish", b"data", "lifecycle"));
        }));
        assert!(result.is_err());
        assert_eq!(fixture.path_call_count(), paths);
        assert_eq!(fixture.seed_file_call_count(), seeds);

        for entry in 0..3 {
            let fixture = AsyncMemoryFixture::new();
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            suite.finish().await;
            let paths = fixture.path_call_count();
            let run = match entry {
                0 => suite.run_check(ContractCheckId::ErrorContext).await,
                1 => suite.run_contract(FileSystemContract::Properties).await,
                _ => suite.run_all().await,
            };
            assert!(!run.failures().is_empty());
            assert!(
                run.failures()[0]
                    .to_string()
                    .contains("session cleanup already started")
            );
            assert_eq!(fixture.path_call_count(), paths);
        }
    });
}

#[test]
fn test_async_cancelled_finish_keeps_preparation_closed_and_can_be_retried() {
    let fixture = PendingTeardownOnce {
        inner: AsyncMemoryFixture::new(),
        calls: AtomicUsize::new(0),
    };
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let mut finish = Box::pin(suite.finish());
    let mut context = Context::from_waker(Waker::noop());
    assert!(Future::poll(Pin::as_mut(&mut finish), &mut context).is_pending());
    drop(finish);

    let calls = fixture.inner.seed_file_call_count();
    let seed = catch_unwind(AssertUnwindSafe(|| {
        run_controlled(suite.required_seed("after-cancel", b"data", "lifecycle"));
    }));
    assert!(seed.is_err());
    assert_eq!(fixture.inner.seed_file_call_count(), calls);
    run_controlled(suite.finish());
}

#[test]
fn test_async_failed_finish_keeps_required_seed_closed_and_can_be_retried() {
    run_controlled(async {
        let fixture = FailAsyncTeardownOnce {
            inner: AsyncMemoryFixture::new(),
            fail: AtomicBool::new(true),
        };
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        assert!(catch_unwind(AssertUnwindSafe(|| run_controlled(suite.finish()))).is_err());
        let paths = fixture.inner.path_call_count();
        let seeds = fixture.inner.seed_file_call_count();
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                run_controlled(suite.required_seed("after-failure", b"data", "lifecycle"));
            }))
            .is_err()
        );
        assert_eq!(fixture.inner.path_call_count(), paths);
        assert_eq!(fixture.inner.seed_file_call_count(), seeds);
        run_controlled(suite.finish());
    });
}

#[test]
fn test_async_seed_then_run_succeeds_and_duplicate_run_is_rejected() {
    run_controlled(async {
        let fixture = AsyncMemoryFixture::new();
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let _path = suite.required_seed("before-run", b"data", "lifecycle").await;
        assert!(
            suite
                .run_contract(FileSystemContract::ErrorContext)
                .await
                .requirements_satisfied()
        );
        assert!(
            !suite
                .run_contract(FileSystemContract::Properties)
                .await
                .failures()
                .is_empty()
        );
    });
}
