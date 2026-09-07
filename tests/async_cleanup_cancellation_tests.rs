// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cancellation must retain resource ownership at every cleanup await point.

#![cfg(feature = "async")]

use std::collections::BTreeSet;
use std::future::Future;
use std::future::poll_fn;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_fs::AsyncFileSystem;
use qubit_fs::FsResult;
use qubit_fs::directory::DeleteOutcome;
use qubit_fs::error::FsError;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileKind;
use qubit_fs::metadata::FileMetadata;
use qubit_fs::metadata::FileSystemCapabilities;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::FileSystemId;
use qubit_fs::metadata::FileSystemInfo;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs::metadata::SymlinkPolicy;
use qubit_fs::path::Path;
use qubit_fs::path::PathConstraints;
use qubit_fs::path::PathSemantics;
use qubit_fs::spi::AsyncFileSystemSpi;
use qubit_fs::spi::DeleteDirectoryRequest;
use qubit_fs::spi::DeleteFileRequest;
use qubit_fs::spi::ProviderOperation;
use qubit_fs::spi::ProviderOperations;
use qubit_fs::spi::ProviderProperties;
use qubit_fs::spi::SpiFuture;
use qubit_fs::spi::StatRequest;
use qubit_fs::spi::StatResponse;
use qubit_fs_testkit as testkit;
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureFuture;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;

/// Shared native state independent of the facade and suite resource ledger.
struct State {
    entries: Mutex<BTreeSet<String>>,
    paused: AtomicBool,
    calls: AtomicUsize,
    pause_at: usize,
    fail_delete_once: AtomicBool,
}

impl State {
    /// Waits at one selected provider call until the test releases the gate.
    async fn checkpoint(&self) {
        let call = self.calls.fetch_add(1, Ordering::Relaxed);
        poll_fn(|_| {
            if call == self.pause_at && self.paused.load(Ordering::Relaxed) {
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        })
        .await;
    }
}

/// Minimal real SPI with pausable metadata and deletion operations.
struct CleanupSpi(Arc<State>);

impl AsyncFileSystemSpi for CleanupSpi {
    fn properties(&self) -> ProviderProperties {
        ProviderProperties::new(
            FileSystemInfo::new(
                FileSystemId::new("cleanup-cancellation").expect("valid filesystem ID"),
                "cleanup-cancellation",
                PathSemantics::Hierarchical,
            ),
            ProviderOperations::new()
                .with(ProviderOperation::Stat)
                .with(ProviderOperation::DeleteFile)
                .with(ProviderOperation::DeleteDirectory),
            FileSystemCapabilities::new().with_guaranteed(FileSystemCapability::Delete),
            FileSystemLimits::unknown(),
            PathConstraints::absolute(),
            SymlinkPolicy::Reject,
        )
        .expect("cleanup SPI capabilities are consistent")
    }

    fn stat<'a>(&'a self, request: StatRequest<'a>) -> SpiFuture<'a, FsResult<StatResponse>> {
        Box::pin(async move {
            self.0.checkpoint().await;
            if self
                .0
                .entries
                .lock()
                .expect("state lock")
                .contains(request.path().as_str())
            {
                Ok(StatResponse::new(
                    request.path().clone(),
                    FileMetadata::new(FileKind::File),
                ))
            } else {
                Err(FsError::new(
                    FsErrorKind::NotFound,
                    FsOperation::Stat,
                    "resource absent",
                ))
            }
        })
    }

    fn delete_file<'a>(&'a self, request: DeleteFileRequest<'a>) -> SpiFuture<'a, FsResult<DeleteOutcome>> {
        Box::pin(async move {
            self.0.checkpoint().await;
            if self.0.fail_delete_once.swap(false, Ordering::Relaxed) {
                return Err(FsError::new(
                    FsErrorKind::PermissionDenied,
                    FsOperation::Delete,
                    "injected delete failure",
                ));
            }
            let removed = self
                .0
                .entries
                .lock()
                .expect("state lock")
                .remove(request.path().as_str());
            Ok(DeleteOutcome::new(!removed))
        })
    }

    fn delete_directory<'a>(&'a self, _request: DeleteDirectoryRequest<'a>) -> SpiFuture<'a, FsResult<DeleteOutcome>> {
        Box::pin(async { panic!("this fixture only owns files") })
    }
}

/// Fixture whose independent teardown exposes resources the ledger missed.
struct CleanupFixture {
    state: Arc<State>,
    filesystem: AsyncFileSystem,
}

impl CleanupFixture {
    /// Creates three resources, with a chosen cleanup call suspended.
    fn new(pause_at: usize) -> Self {
        let state = Arc::new(State {
            entries: Mutex::new(BTreeSet::new()),
            paused: AtomicBool::new(true),
            calls: AtomicUsize::new(0),
            pause_at,
            fail_delete_once: AtomicBool::new(false),
        });
        let filesystem = AsyncFileSystem::from_spi(CleanupSpi(Arc::clone(&state))).expect("valid cleanup facade");
        Self { state, filesystem }
    }
}

impl AsyncFileSystemFixture for CleanupFixture {
    fn file_system(&self) -> &AsyncFileSystem {
        &self.filesystem
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/{relative}")).map_err(|error| FixtureError::with_source("fixture path", error))
    }

    fn seed_file<'a>(&'a self, relative: &'a str, _bytes: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<Path>> {
        Box::pin(async move {
            let path = self.path(relative)?;
            self.state
                .entries
                .lock()
                .expect("state lock")
                .insert(path.as_str().to_owned());
            Ok(FixtureSupport::Supported(path))
        })
    }

    fn teardown(&self) -> FixtureFuture<'_, ()> {
        Box::pin(async move {
            let mut entries = self.state.entries.lock().expect("state lock");
            let remaining = entries.len();
            entries.clear();
            if remaining == 0 {
                Ok(())
            } else {
                Err(FixtureError::new(format!("facade cleanup lost {remaining} resources")))
            }
        })
    }
}

/// Polls a future that must finish after the provider gate has been released.
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("released cleanup must finish without another suspension"),
    }
}

/// Cancellation at inspection, deletion, or verification retains all entries.
#[test]
fn test_finish_retries_every_resource_after_cancellation() {
    // First resource: stat=0, delete=1, verification=2; second starts at 3.
    for pause_at in 0..6 {
        let fixture = CleanupFixture::new(pause_at);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        for name in ["first", "second", "third"] {
            ready(suite.required_seed(name, b"owned", "cleanup cancellation"));
        }
        let mut cleanup = Box::pin(suite.finish());
        assert!(
            matches!(
                cleanup.as_mut().poll(&mut Context::from_waker(Waker::noop())),
                Poll::Pending
            ),
            "cleanup did not reach pause {pause_at}"
        );
        drop(cleanup);
        fixture.state.paused.store(false, Ordering::Relaxed);
        ready(suite.finish());
        assert!(fixture.state.entries.lock().expect("state lock").is_empty());
    }
}

/// Dropping the runner keeps pre-registered evidence and ends the session.
#[test]
fn test_cancelled_run_retains_report_and_rejects_restart() {
    let fixture = CleanupFixture::new(0);
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let mut execution = Box::pin(suite.run_contract(testkit::FileSystemContract::ErrorContext));
    assert!(matches!(
        execution.as_mut().poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    drop(execution);
    assert!(suite.run().was_interrupted());
    assert_eq!(suite.run().report().checks().len(), 1);
    assert!(!suite.run().requirements_satisfied());
    let calls = fixture.state.calls.load(Ordering::Relaxed);
    let result = ready(suite.run_contract(testkit::FileSystemContract::ErrorContext));
    assert!(!result.failures().is_empty());
    assert_eq!(fixture.state.calls.load(Ordering::Relaxed), calls);
    fixture.state.paused.store(false, Ordering::Relaxed);
    ready(suite.finish());
}

/// A later cancelled await cannot erase a failure already observed.
#[test]
fn test_cleanup_failure_survives_later_cancellation() {
    let fixture = CleanupFixture::new(2);
    fixture.state.fail_delete_once.store(true, Ordering::Relaxed);
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    for name in ["first", "second"] {
        ready(suite.required_seed(name, b"owned", "failure retention"));
    }
    let mut cleanup = Box::pin(suite.finish());
    assert!(matches!(
        cleanup.as_mut().poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    drop(cleanup);
    assert_eq!(suite.run().cleanup().failures().len(), 1);
    fixture.state.paused.store(false, Ordering::Relaxed);
    ready(suite.finish());
    assert_eq!(suite.run().cleanup().failures().len(), 1);
    assert!(suite.run().cleanup().completed());
}
