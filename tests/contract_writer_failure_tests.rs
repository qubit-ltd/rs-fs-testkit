// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Recovery ownership remains available through a borrowed contract result.

use qubit_fs as qfs;
#[cfg(feature = "async")]
use qubit_fs::write::AsyncWriterRecovery;

mod common;
use qubit_fs::error::FsError;
use qubit_fs::error::FsErrorKind;
use qubit_fs::write::FileWriter;
use qubit_fs::write::WriteAbortOutcome;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractSource;
use qubit_fs_testkit::ContractWriterFailure;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;

use self::common::MemoryFault;
use self::common::MemoryFixture;
/// Operation failure must retain the operation that owns the recovery writer.
#[cfg(feature = "async")]
#[test]
fn test_async_owning_failure_retains_operation() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;
    use qubit_fs_testkit::ContractAsyncWriteFailure;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    for (fault, check) in [
        (
            AsyncMemoryFault::OwningCommitFails,
            ContractCheckId::WriteOwningOperation,
        ),
        (AsyncMemoryFault::ReplaceCommitFails, ContractCheckId::WriteReplace),
    ] {
        let fixture = AsyncMemoryFixture::with_fault(fault);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_contract(FileSystemContract::Write).await;
            let failure = &run.failures()[0];
            assert_eq!(failure.check(), Some(check));
            let retained = std::error::Error::source(failure)
                .expect("retained source")
                .downcast_ref::<ContractSource>()
                .expect("owned source");
            let mut original = retained
                .take()
                .expect("recovery ownership")
                .downcast::<ContractAsyncWriteFailure>()
                .expect("operation must remain available");
            assert_eq!(original.error().kind(), FsErrorKind::PermissionDenied);
            assert_eq!(
                original.failure().state(),
                qfs::write::WriteFailureState::RetryableNotPublished
            );
            assert!(original.failure().written_bytes() > 0);
            assert!(original.operation().expect("retained operation").has_recovery());
            let operation = original.operation_mut().expect("execution started");
            let mut writer = operation
                .take_recovery()
                .map(|recovery| match recovery {
                    AsyncWriterRecovery::Opened(writer) => writer,
                    AsyncWriterRecovery::Rejected(_) => {
                        panic!("fixture must return a validated identity")
                    }
                })
                .expect("recovery writer");
            assert_eq!(
                writer.abort_async().await.expect("explicit abort"),
                WriteAbortOutcome::NotPublished
            );
            let (snapshot, operation) = original.into_parts();
            assert_eq!(snapshot.error().kind(), FsErrorKind::PermissionDenied);
            assert!(!operation.expect("operation remains owned").has_recovery());
            assert!(retained.take().is_none());
            assert!(!run.requirements_satisfied());
        });
    }
}

/// A failed basic commit must preserve its writer, not just the commit error.
#[cfg(feature = "async")]
#[test]
fn test_async_basic_commit_failure_retains_writer() {
    use qubit_fs::write::AsyncFileWriter;
    use qubit_fs::write::WriteFailure;
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::BasicCommitFails);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(FileSystemContract::Write).await;
        let failure = &run.failures()[0];
        assert_eq!(failure.check(), Some(ContractCheckId::WriteBasic));
        let retained = std::error::Error::source(failure)
            .expect("retained source")
            .downcast_ref::<ContractSource>()
            .expect("owned source");
        let mut original = retained
            .take()
            .expect("recovery ownership")
            .downcast::<ContractWriterFailure<AsyncFileWriter>>()
            .expect("commit failure must retain writer");
        assert_eq!(
            original
                .error()
                .downcast_ref::<WriteFailure>()
                .expect("original commit failure")
                .error()
                .kind(),
            FsErrorKind::PermissionDenied
        );
        assert_eq!(
            original.writer_mut().abort_async().await.expect("explicit abort"),
            WriteAbortOutcome::NotPublished
        );
        assert!(!run.requirements_satisfied());
    });
}

/// A caller can take the failed writer, retry abort, and inspect its original
/// error.
#[test]
fn test_failed_abort_retains_writer_for_explicit_retry() {
    let fixture = MemoryFixture::with_fault(MemoryFault::AbortFailsOnce);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Write);
    let failure = &run.failures()[0];
    assert_eq!(failure.check(), Some(ContractCheckId::WriteAbort));
    let retained = std::error::Error::source(failure)
        .expect("retained source")
        .downcast_ref::<ContractSource>()
        .expect("owned source");
    let mut original = retained
        .take()
        .expect("recovery ownership")
        .downcast::<ContractWriterFailure<FileWriter>>()
        .expect("original writer type");
    assert_eq!(
        original
            .error()
            .downcast_ref::<FsError>()
            .expect("original filesystem error")
            .kind(),
        FsErrorKind::PermissionDenied
    );
    assert_eq!(
        original.writer_mut().abort().expect("retry abort"),
        WriteAbortOutcome::NotPublished
    );
    assert_eq!(original.writer().state(), qfs::write::WriterState::Aborted);
    let (error, writer) = original.into_parts();
    assert!(error.downcast_ref::<FsError>().is_some());
    assert_eq!(writer.state(), qfs::write::WriterState::Aborted);
    assert!(retained.take().is_none());
    assert!(
        !run.requirements_satisfied(),
        "recovery does not erase observed failure"
    );
}

/// Async cleanup failure transfers a usable writer without losing its source.
#[cfg(feature = "async")]
#[test]
fn test_async_failed_abort_retains_writer_for_explicit_retry() {
    use qubit_fs::write::AsyncFileWriter;
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::AbortFailsOnce);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(FileSystemContract::Write).await;
        let failure = &run.failures()[0];
        assert_eq!(failure.check(), Some(ContractCheckId::WriteAbort));
        let retained = std::error::Error::source(failure)
            .expect("retained source")
            .downcast_ref::<ContractSource>()
            .expect("owned source");
        let mut original = retained
            .take()
            .expect("recovery ownership")
            .downcast::<ContractWriterFailure<AsyncFileWriter>>()
            .expect("original writer type");
        assert_eq!(
            original
                .error()
                .downcast_ref::<FsError>()
                .expect("original filesystem error")
                .kind(),
            FsErrorKind::PermissionDenied
        );
        assert_eq!(
            original.writer_mut().abort_async().await.expect("retry abort"),
            WriteAbortOutcome::NotPublished
        );
        assert!(retained.take().is_none());
        assert!(!run.requirements_satisfied());
    });
}
