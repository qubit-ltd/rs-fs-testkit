// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Cancellation failures transfer usable recovery ownership through the report.
#![cfg(feature = "async")]

use qubit_fs::write::AsyncWriterRecovery;
use qubit_fs_testkit as testkit;

mod common;
use qubit_fs::write::AsyncFileWriter;
use qubit_fs::write::WriteAbortOutcome;
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::ContractAsyncCopyFailure;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractSource;
use qubit_fs_testkit::ContractWriterFailure;

use self::common::AsyncMemoryFault;
use self::common::AsyncMemoryFixture;
use self::common::async_memory_file_system::run_controlled;
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
            let source = std::error::Error::source(failure)
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
            let source = std::error::Error::source(failure)
                .expect("source")
                .downcast_ref::<ContractSource>()
                .expect("operation source");
            let mut retained = source
                .take()
                .expect("operation ownership")
                .downcast::<ContractAsyncCopyFailure>()
                .expect("copy failure");
            assert_eq!(retained.failure().partial_stats().bytes, 0);
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
        let source = std::error::Error::source(failure)
            .expect("source")
            .downcast_ref::<ContractSource>()
            .expect("operation source");
        let mut retained = source
            .take()
            .expect("operation ownership")
            .downcast::<testkit::ContractAsyncWriteFailure>()
            .expect("copy failure");
        assert_eq!(retained.failure().written_bytes(), 0);
        assert!(retained.operation().expect("admitted operation").has_recovery());
        let mut writer = retained
            .operation_mut()
            .expect("admitted operation")
            .take_recovery()
            .map(|recovery| match recovery {
                AsyncWriterRecovery::Opened(writer) => writer,
                AsyncWriterRecovery::Rejected(_) => panic!("fixture must return a validated identity"),
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
