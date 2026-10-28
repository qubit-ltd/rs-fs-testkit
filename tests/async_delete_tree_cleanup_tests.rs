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
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Wake;
use std::task::Waker;

pub use ::qubit_fs_testkit;
use qubit_fs::AsyncFileSystem;
use qubit_fs::path::Path;
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureFuture;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;

use self::common::async_memory_file_system::AsyncMemoryFault;
use self::common::async_memory_file_system::AsyncMemoryFixture;
use self::common::async_memory_file_system::run_controlled;

enum SeedBehavior {
    None,
    FailDirectory(usize),
    FailFile(usize),
    PendingFile(usize),
}

struct DeleteTreeFixture {
    inner: AsyncMemoryFixture,
    behavior: SeedBehavior,
    directory_calls: AtomicUsize,
    file_calls: AtomicUsize,
}

impl DeleteTreeFixture {
    fn new(behavior: SeedBehavior) -> Self {
        Self {
            inner: AsyncMemoryFixture::with_fault(AsyncMemoryFault::CleanupDeleteError),
            behavior,
            directory_calls: AtomicUsize::new(0),
            file_calls: AtomicUsize::new(0),
        }
    }
}

impl AsyncFileSystemFixture for DeleteTreeFixture {
    fn file_system(&self) -> &AsyncFileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn seed_empty_directory<'a>(&'a self, relative: &'a str) -> FixtureFuture<'a, FixtureSupport<Path>> {
        Box::pin(async move {
            let call = self.directory_calls.fetch_add(1, Ordering::Relaxed) + 1;
            if matches!(self.behavior, SeedBehavior::FailDirectory(failed) if failed == call) {
                return Err(FixtureError::new("injected second directory seed failure"));
            }
            self.inner.seed_empty_directory(relative).await
        })
    }

    fn seed_file<'a>(&'a self, relative: &'a str, bytes: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<Path>> {
        Box::pin(async move {
            let call = self.file_calls.fetch_add(1, Ordering::Relaxed) + 1;
            if matches!(self.behavior, SeedBehavior::FailFile(failed) if failed == call) {
                return Err(FixtureError::new("injected first file seed failure"));
            }
            let seeded = self.inner.seed_file(relative, bytes).await?;
            if matches!(self.behavior, SeedBehavior::PendingFile(pending) if pending == call) {
                std::future::pending::<()>().await;
            }
            Ok(seeded)
        })
    }

    fn teardown(&self) -> FixtureFuture<'_, ()> {
        self.inner.teardown()
    }
}

struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: std::sync::Arc<Self>) {}
}

fn poll_once<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    let waker = Waker::from(std::sync::Arc::new(NoopWake));
    future.poll(&mut Context::from_waker(&waker))
}

fn assert_cleanup_recorded(fixture: &DeleteTreeFixture) {
    let mut suite = AsyncFileSystemContractSuite::new(fixture);
    let run = run_controlled(suite.run_check(ContractCheckId::DeleteTree));
    assert!(
        !run.failures().is_empty(),
        "the injected contract failure must be retained"
    );
    let cleanup = run.cleanup();
    assert!(
        !cleanup.failures().is_empty(),
        "prepared paths must reach facade cleanup"
    );
    assert!(
        cleanup
            .failures()
            .iter()
            .any(|failure| failure.to_string().contains("facade cleanup operation failed")),
        "cleanup must retain the injected facade deletion failure"
    );
}

#[test]
fn test_async_delete_tree_second_directory_seed_failure_keeps_first_path_for_cleanup() {
    assert_cleanup_recorded(&DeleteTreeFixture::new(SeedBehavior::FailDirectory(2)));
}

#[test]
fn test_async_delete_tree_first_file_seed_failure_keeps_directories_for_cleanup() {
    assert_cleanup_recorded(&DeleteTreeFixture::new(SeedBehavior::FailFile(1)));
}

#[test]
fn test_async_delete_tree_deletion_failure_keeps_seeded_paths_for_cleanup() {
    assert_cleanup_recorded(&DeleteTreeFixture::new(SeedBehavior::None));
}

#[test]
fn test_async_delete_tree_cancelled_after_seed_can_finish_with_cleanup_evidence() {
    let fixture = DeleteTreeFixture::new(SeedBehavior::PendingFile(1));
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let mut execution = Box::pin(suite.run_check(ContractCheckId::DeleteTree));
    assert!(matches!(poll_once(execution.as_mut()), Poll::Pending));
    drop(execution);
    assert!(suite.run().was_interrupted());

    let finish = std::panic::catch_unwind(AssertUnwindSafe(|| run_controlled(suite.finish())));
    assert!(finish.is_err(), "the configured facade cleanup failure must be visible");
    assert!(
        !suite.run().cleanup().failures().is_empty(),
        "paths seeded before cancellation must remain in the cleanup ledger"
    );
}
