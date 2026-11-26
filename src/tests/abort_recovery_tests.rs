// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Recovery ownership when an explicit asynchronous writer operation is
//! dropped.

use std::error::Error;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_fs::AsyncFileSystem;
use qubit_fs::path::Path;
use qubit_fs::write::AsyncFileWriter;
use qubit_fs::write::WriteAbortOutcome;
use qubit_fs::write::WriterState;

use super::rustdoc_support::fs_rustdoc_support::async_recording_spi::AsyncCopyStage;
use super::rustdoc_support::fs_rustdoc_support::async_recording_spi::AsyncRecordingConfig;
use super::rustdoc_support::fs_rustdoc_support::async_recording_spi::async_recording_file_system;
use crate::AsyncFileSystemContractSuite;
use crate::AsyncFileSystemFixture;
use crate::ContractCheckId;
use crate::ContractSource;
use crate::ContractWriterFailure;
use crate::FixtureError;
use crate::FixtureFuture;
use crate::FixtureResult;
use crate::FixtureSupport;

struct RecordingFixture {
    file_system: AsyncFileSystem,
}

impl AsyncFileSystemFixture for RecordingFixture {
    fn file_system(&self) -> &AsyncFileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/abort-recovery/{relative}"))
            .map_err(|error| FixtureError::with_source("invalid test path", error))
    }

    fn exists_out_of_band<'a>(&'a self, _path: &'a Path) -> FixtureFuture<'a, FixtureSupport<bool>> {
        Box::pin(async { Ok(FixtureSupport::Supported(false)) })
    }

    fn teardown(&self) -> FixtureFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}

fn poll_once<F: Future>(future: std::pin::Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn test_abort_future_drop_retains_writer_for_explicit_retry() {
    let release_abort = Arc::new(AtomicBool::new(false));
    let (file_system, _) = async_recording_file_system(AsyncRecordingConfig {
        pending_stage: Some(AsyncCopyStage::WriterAbort),
        writer_abort_release: Some(Arc::clone(&release_abort)),
        ..AsyncRecordingConfig::default()
    });
    let fixture = RecordingFixture { file_system };
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let mut run_check = Box::pin(suite.run_check(ContractCheckId::WriteAbort));
    assert!(matches!(poll_once(run_check.as_mut()), Poll::Pending));
    drop(run_check);

    let failure = suite
        .run()
        .cleanup()
        .failures()
        .first()
        .expect("dropped abort must retain its writer");
    assert_eq!(failure.check(), Some(ContractCheckId::WriteAbort));
    let source = Error::source(failure)
        .and_then(|source| source.downcast_ref::<ContractSource>())
        .expect("retained writer source");
    let retained = source.take().expect("writer ownership transfers exactly once");
    let mut retained = retained
        .downcast::<ContractWriterFailure<AsyncFileWriter>>()
        .unwrap_or_else(|_| panic!("expected retained async file writer"));

    release_abort.store(true, Ordering::Relaxed);
    let mut retry = Box::pin(retained.writer_mut().abort_async());
    assert!(matches!(
        poll_once(retry.as_mut()),
        Poll::Ready(Ok(WriteAbortOutcome::NotPublished))
    ));
    drop(retry);
    assert_eq!(retained.writer().state(), WriterState::Aborted);
}

#[test]
fn test_pending_abort_write_after_open_retains_writer_for_explicit_retry() {
    let (file_system, _) = async_recording_file_system(AsyncRecordingConfig {
        pending_stage: Some(AsyncCopyStage::WriterWrite),
        ..AsyncRecordingConfig::default()
    });
    let fixture = RecordingFixture { file_system };
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let mut run_check = Box::pin(suite.run_check(ContractCheckId::WriteAbort));
    assert!(matches!(poll_once(run_check.as_mut()), Poll::Pending));
    drop(run_check);

    let failure = suite
        .run()
        .cleanup()
        .failures()
        .first()
        .expect("dropping a pending write after open must retain its writer");
    assert_eq!(failure.check(), Some(ContractCheckId::WriteAbort));
    let source = Error::source(failure)
        .and_then(|source| source.downcast_ref::<ContractSource>())
        .expect("retained writer source");
    let retained = source.take().expect("writer ownership transfers exactly once");
    let mut retained = retained
        .downcast::<ContractWriterFailure<AsyncFileWriter>>()
        .unwrap_or_else(|_| panic!("expected retained async file writer"));
    let mut retry = Box::pin(retained.writer_mut().abort_async());
    assert!(matches!(
        poll_once(retry.as_mut()),
        Poll::Ready(Ok(WriteAbortOutcome::NotPublished))
    ));
    drop(retry);
    assert_eq!(retained.writer().state(), WriterState::Aborted);
}
