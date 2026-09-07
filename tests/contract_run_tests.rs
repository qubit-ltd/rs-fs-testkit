// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! A suite retains its completed result and rejects another execution session.

use qubit_fs as qfs;
use qubit_fs_testkit as testkit;

mod common;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;

use self::common::MemoryFault;
use self::common::MemoryFixture;
#[test]
fn test_result_remains_in_the_suite() {
    let fixture = MemoryFixture::new();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::ErrorContext);
    assert!(run.requirements_satisfied());
    assert!(run.failures().is_empty());
    assert!(suite.run().requirements_satisfied());
}

#[test]
fn test_failed_run_returns_evidence_without_unwinding() {
    let fixture = MemoryFixture::with_fault(MemoryFault::ReadWrongBytes);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Read);
    assert!(!run.requirements_satisfied());
    assert!(!run.failures().is_empty());
    assert_eq!(run.failures()[0].check(), Some(testkit::ContractCheckId::ReadBasic));
    assert!(
        run.report()
            .checks()
            .iter()
            .any(|check| check.id() == testkit::ContractCheckId::ReadBasic
                && matches!(check.outcome(), testkit::ContractCheckOutcome::Failed { .. }))
    );
}

#[test]
fn test_completed_session_cannot_be_reused() {
    let fixture = MemoryFixture::new();
    let mut suite = FileSystemContractSuite::new(&fixture);
    assert!(
        suite
            .run_contract(FileSystemContract::ErrorContext)
            .requirements_satisfied()
    );
    let run = suite.run_contract(FileSystemContract::Read);
    assert!(!run.requirements_satisfied());
    assert_eq!(run.report().checks().len(), 1);
}

#[test]
fn test_hierarchical_literal_prefix_requires_rejection_evidence() {
    let fixture = MemoryFixture::new();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::List);
    assert!(run.requirements_satisfied());
    let check = run
        .report()
        .checks()
        .iter()
        .find(|check| check.id() == testkit::ContractCheckId::ListLiteralPrefix)
        .expect("registered literal prefix");
    assert!(matches!(
        check.outcome(),
        testkit::ContractCheckOutcome::RejectedAsExpected
    ));
}

#[test]
fn test_repeated_temp_lifecycle_requires_real_evidence() {
    let fixture = MemoryFixture::new();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::TempResources);
    assert!(run.requirements_satisfied());
    let check = run
        .report()
        .checks()
        .iter()
        .find(|check| check.id() == testkit::ContractCheckId::TempRepeatedLifecycle)
        .expect("registered lifecycle");
    assert!(matches!(check.outcome(), testkit::ContractCheckOutcome::Passed));
}

/// Whole-file write errors retain their non-Sync recovery handle without
/// making the run itself unsuitable for sharing between threads.
#[test]
fn test_write_failure_preserves_owned_recovery_source() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<testkit::ContractRun>();
    let fixture = MemoryFixture::with_fault(MemoryFault::WriteCommitFailure);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Write);
    let failure = &run.failures()[0];
    assert_eq!(failure.check(), Some(testkit::ContractCheckId::WriteBasic));
    let source = std::error::Error::source(failure).expect("original write error");
    let retained = source
        .downcast_ref::<testkit::ContractSource>()
        .expect("owned source wrapper");
    retained
        .inspect(|error| {
            let write = error
                .downcast_ref::<qfs::write::WriteAllFailure>()
                .expect("typed whole-write failure");
            assert!(write.writer().is_some(), "recovery writer must be retained");
        })
        .expect("source has not been taken");
    let mut original = retained
        .take()
        .expect("take original error")
        .downcast::<qfs::write::WriteAllFailure>()
        .expect("original type remains intact");
    let outcome = original
        .writer_mut()
        .expect("retained recovery writer")
        .abort()
        .expect("explicit recovery");
    assert_eq!(outcome, qfs::write::WriteAbortOutcome::NotPublished);
    assert!(retained.inspect(|_| ()).is_none(), "source ownership transfers once");
    assert!(
        !run.requirements_satisfied(),
        "recovery must not erase the contract failure"
    );
}

/// Owning-operation admission errors retain their check identity and source.
#[cfg(feature = "async")]
#[test]
fn test_async_owning_write_failure_is_structured() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;
    use qubit_fs_testkit::ContractCheckId;
    use qubit_fs_testkit::ContractCheckOutcome;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::new().with_invalid_probe_path("owning-write");
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(FileSystemContract::Write).await;
        let failure = run.failures().first().expect("invalid owning request must fail");
        assert_eq!(failure.check(), Some(ContractCheckId::WriteOwningOperation));
        assert!(
            std::error::Error::source(failure).is_some(),
            "original failure must be retained"
        );
        assert!(
            run.report()
                .checks()
                .iter()
                .any(|check| check.id() == ContractCheckId::WriteOwningOperation
                    && matches!(check.outcome(), ContractCheckOutcome::Failed { .. }))
        );
        assert!(run.cleanup().attempts() > 0);
        assert!(
            !run.cleanup().failures().is_empty(),
            "invalid path cleanup failure must also survive"
        );
        assert!(fixture.is_empty(), "independent teardown must reclaim valid resources");
    });
}
