// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cancellation failures transfer usable recovery ownership through the report.
#![cfg(feature = "async")]

use std::error::Error;

pub use ::qubit_fs_testkit;
use qubit_fs::copy::AsyncCopyOperation;
use qubit_fs::copy::AsyncCopyOperationState;
use qubit_fs::copy::CopyFailureState;
use qubit_fs::error::FsErrorKind;
use qubit_fs::write::AsyncFileWriter;
use qubit_fs::write::AsyncWriteAllOperation;
use qubit_fs::write::AsyncWriteAllOperationState;
use qubit_fs::write::AsyncWriterRecovery;
use qubit_fs::write::WriteAbortOutcome;
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::ContractAsyncCopyFailure;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractFailure;
use qubit_fs_testkit::ContractSource;
use qubit_fs_testkit::ContractWriterFailure;

use self::common::AsyncMemoryFault;
use self::common::AsyncMemoryFixture;
use self::common::async_memory_file_system::run_controlled;
use crate::qubit_fs_testkit as testkit;

mod common;

/// Diagnostic failures after cancellation retain operations that still own
/// writers.
#[test]
fn test_write_cancellation_diagnostic_failures_retain_operation() {
    for (fault, check) in [
        (
            AsyncMemoryFault::WriteCancelAcknowledgeFails,
            ContractCheckId::WriteCancelCommit,
        ),
        (
            AsyncMemoryFault::WriteCancelDisarmFails,
            ContractCheckId::WriteCancelCommit,
        ),
        (
            AsyncMemoryFault::WriteCancelAcceptedBytesFails,
            ContractCheckId::WriteCancelCommit,
        ),
    ] {
        let fixture = AsyncMemoryFixture::with_fault(fault);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(check).await;
            let failure = run.failures().first().expect("diagnostic failure");
            let source = Error::source(failure)
                .and_then(|source| source.downcast_ref::<ContractSource>())
                .expect("owned operation source");
            let mut retained = source
                .take()
                .expect("operation source can be transferred")
                .downcast::<ContractWriterFailure<AsyncWriteAllOperation>>()
                .expect("write operation failure");
            assert!(retained.error().downcast_ref::<ContractFailure>().is_some());
            assert_eq!(
                retained.writer().state(),
                AsyncWriteAllOperationState::Failed(qubit_fs::write::WriteFailureState::Indeterminate)
            );
            let operation = retained.writer_mut();
            assert!(operation.has_recovery());
            let mut writer = operation
                .take_recovery()
                .and_then(|recovery| match recovery {
                    AsyncWriterRecovery::Opened(writer) => Some(*writer),
                    AsyncWriterRecovery::Rejected(_) => None,
                })
                .expect("recovery writer remains owned");
            assert_eq!(
                writer.abort_async().await.expect("explicit abort"),
                WriteAbortOutcome::NotPublished
            );
            assert!(!run.requirements_satisfied());
        });
    }
}

/// Copy acknowledgement and disarm failures retain an opened destination
/// writer.
#[test]
fn test_copy_cancellation_diagnostic_failures_retain_operation() {
    for fault in [
        AsyncMemoryFault::CopyCancelAcknowledgeFails,
        AsyncMemoryFault::CopyCancelDisarmFails,
    ] {
        let fixture = AsyncMemoryFixture::with_fault(fault);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::AsyncCopyCancelCommit).await;
            let failure = run.failures().first().expect("diagnostic failure");
            let source = Error::source(failure)
                .and_then(|source| source.downcast_ref::<ContractSource>())
                .expect("owned operation source");
            let mut retained = source
                .take()
                .expect("operation source can be transferred")
                .downcast::<ContractWriterFailure<AsyncCopyOperation>>()
                .expect("copy operation failure");
            assert!(retained.error().downcast_ref::<ContractFailure>().is_some());
            assert_eq!(
                retained.writer().state(),
                AsyncCopyOperationState::Failed(CopyFailureState::Indeterminate)
            );
            let operation = retained.writer_mut();
            assert!(operation.has_recovery());
            let mut writer = operation
                .take_recovery()
                .and_then(|recovery| match recovery {
                    AsyncWriterRecovery::Opened(writer) => Some(*writer),
                    AsyncWriterRecovery::Rejected(_) => None,
                })
                .expect("recovery writer remains owned");
            assert_eq!(
                writer.abort_async().await.expect("explicit abort"),
                WriteAbortOutcome::NotPublished
            );
            assert!(!run.requirements_satisfied());
        });
    }
}

/// Diagnostic failures before a writer opens still retain operation state.
#[test]
fn test_cancellation_diagnostic_failures_retain_operation_without_writer() {
    for (fault, check) in [
        (
            AsyncMemoryFault::WriteCancelAcknowledgeFails,
            ContractCheckId::WriteCancelOpen,
        ),
        (
            AsyncMemoryFault::CopyCancelAcknowledgeFails,
            ContractCheckId::AsyncCopyCancelNativeAttempt,
        ),
    ] {
        let fixture = AsyncMemoryFixture::with_fault(fault);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(check).await;
            let failure = run.failures().first().expect("diagnostic failure");
            let source = Error::source(failure)
                .and_then(|source| source.downcast_ref::<ContractSource>())
                .expect("owned operation source");
            let source = source.take().expect("operation source can be transferred");
            match check {
                ContractCheckId::WriteCancelOpen => {
                    let retained = source
                        .downcast::<ContractWriterFailure<AsyncWriteAllOperation>>()
                        .expect("write operation failure");
                    assert!(!retained.writer().has_recovery());
                }
                ContractCheckId::AsyncCopyCancelNativeAttempt => {
                    let retained = source
                        .downcast::<ContractWriterFailure<AsyncCopyOperation>>()
                        .expect("copy operation failure");
                    assert!(!retained.writer().has_recovery());
                }
                _ => unreachable!("test only selects no-writer stages"),
            }
            assert!(!run.requirements_satisfied());
        });
    }
}

/// Both probe drivers retain a writer when explicit recovery abort fails.
#[test]
fn test_failed_cancellation_abort_retains_writer() {
    run_controlled(async {
        for id in [
            ContractCheckId::WriteCancelCommit,
            ContractCheckId::AsyncCopyCancelCommit,
        ] {
            let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::RecoveryAbortFailsOnce);
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            let failure = &run.failures()[0];
            assert_eq!(failure.check(), Some(id));
            let source = Error::source(failure)
                .expect("source")
                .downcast_ref::<ContractSource>()
                .expect("recovery source");
            let mut retained = source
                .take()
                .expect("writer ownership")
                .downcast::<ContractWriterFailure<AsyncFileWriter>>()
                .expect("writer failure");
            assert_eq!(
                retained.writer_mut().abort_async().await.expect("retry abort"),
                WriteAbortOutcome::NotPublished
            );
            assert!(source.take().is_none());
            assert!(!run.requirements_satisfied());
        }
    });
}

/// An execution error before acknowledgement retains the copy operation and
/// writer.
#[test]
fn test_copy_error_before_requested_stage_retains_operation() {
    for id in [
        ContractCheckId::AsyncCopyCancelCommit,
        ContractCheckId::CopyBasic,
        ContractCheckId::CopyRepeatedExecute,
    ] {
        let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::CopyWriteFails);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            let failure = &run.failures()[0];
            assert_eq!(failure.check(), Some(id));
            let source = Error::source(failure)
                .expect("source")
                .downcast_ref::<ContractSource>()
                .expect("operation source");
            let mut retained = source
                .take()
                .expect("operation ownership")
                .downcast::<ContractAsyncCopyFailure>()
                .expect("copy failure");
            assert_eq!(retained.error().kind(), FsErrorKind::PermissionDenied);
            assert_eq!(retained.failure().partial_stats().bytes, 0);
            assert!(format!("{retained:?}").contains("has_operation"));
            assert!(format!("{retained}").contains("original failure"));
            assert!(Error::source(&*retained).is_some());
            assert!(retained.operation().expect("admitted operation").has_recovery());
            let mut writer = retained
                .operation_mut()
                .expect("admitted operation")
                .take_recovery()
                .map(|recovery| match recovery {
                    AsyncWriterRecovery::Opened(writer) => writer,
                    AsyncWriterRecovery::Rejected(_) => {
                        panic!("fixture must return a validated identity")
                    }
                })
                .expect("retained writer");
            assert_eq!(
                writer.abort_async().await.expect("explicit abort"),
                WriteAbortOutcome::NotPublished
            );
            let (snapshot, operation) = retained.into_parts();
            assert_eq!(snapshot.partial_stats().bytes, 0);
            assert!(!operation.expect("operation remains").has_recovery());
            assert!(!run.requirements_satisfied());
        });
    }
}

/// An execution error before acknowledgement retains the write operation and
/// writer.
#[test]
fn test_write_error_before_requested_stage_retains_operation() {
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::CopyWriteFails);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::WriteCancelCommit).await;
        let failure = &run.failures()[0];
        assert_eq!(failure.check(), Some(ContractCheckId::WriteCancelCommit));
        let source = Error::source(failure)
            .expect("source")
            .downcast_ref::<ContractSource>()
            .expect("operation source");
        let mut retained = source
            .take()
            .expect("operation ownership")
            .downcast::<testkit::ContractAsyncWriteFailure>()
            .expect("copy failure");
        assert_eq!(retained.error().kind(), FsErrorKind::PermissionDenied);
        assert_eq!(retained.failure().written_bytes(), 0);
        assert!(format!("{retained:?}").contains("has_operation"));
        assert!(format!("{retained}").contains("original failure"));
        assert!(Error::source(&*retained).is_some());
        assert!(retained.operation().expect("admitted operation").has_recovery());
        let mut writer = retained
            .operation_mut()
            .expect("admitted operation")
            .take_recovery()
            .map(|recovery| match recovery {
                AsyncWriterRecovery::Opened(writer) => writer,
                AsyncWriterRecovery::Rejected(_) => {
                    panic!("fixture must return a validated identity")
                }
            })
            .expect("retained writer");
        assert_eq!(
            writer.abort_async().await.expect("explicit abort"),
            WriteAbortOutcome::NotPublished
        );
        let (snapshot, operation) = retained.into_parts();
        assert_eq!(snapshot.written_bytes(), 0);
        assert!(!operation.expect("operation remains").has_recovery());
        assert!(!run.requirements_satisfied());
    });
}
