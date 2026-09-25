// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

#![cfg(feature = "async")]

use qubit_fs_testkit as testkit;

mod common;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::FileSystemContract;

use self::common::AsyncMemoryFixture;
use self::common::async_memory_file_system::run_controlled;
/// Every stage is acknowledged after multiple real provider suspensions.
#[test]
fn test_async_copy_cancellation_waits_for_each_stage() {
    let fixture = AsyncMemoryFixture::new();
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        suite.run_contract(FileSystemContract::Copy).await.assert_satisfied();
    });
    assert!(fixture.is_empty(), "cancellation probes must be cleaned");
}

/// Dropping an in-flight assertion does not start asynchronous cleanup.
#[test]
fn test_async_copy_cancellation_drop_leaves_explicit_teardown_responsibility() {
    let fixture = AsyncMemoryFixture::new();
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let mut assertion = Box::pin(suite.run_contract(FileSystemContract::Copy));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(assertion.as_mut().poll(&mut context), Poll::Pending));
    drop(assertion);

    run_controlled(fixture.teardown()).expect("explicit fixture teardown must succeed");
    assert!(fixture.is_empty(), "explicit fixture teardown must reclaim data");
}

/// Four stage gates report independently counted accepted bytes.
#[test]
fn test_async_write_cancellation_verifies_every_stage() {
    let fixture = AsyncMemoryFixture::new();
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        suite.run_contract(FileSystemContract::Write).await.assert_satisfied();
    });
    assert!(fixture.is_empty(), "write cancellation must release all resources");
    assert_eq!(
        fixture.write_cancellation_stages(),
        [
            testkit::AsyncWriteCancellationStage::Open,
            testkit::AsyncWriteCancellationStage::Write,
            testkit::AsyncWriteCancellationStage::Flush,
            testkit::AsyncWriteCancellationStage::Commit,
        ]
    );
}

/// Callers can require a normally optional probe without weakening cleanup
/// checks.
#[test]
fn test_run_can_require_optional_cancellation_evidence() {
    let fixture = AsyncMemoryFixture::without_cancellation_cases();
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(testkit::FileSystemContract::Copy).await;
        assert!(run.requirements_satisfied());
        assert!(!run.all_applicable_checks_verified());
        assert!(!run.requirements_satisfied_with(&[testkit::ContractCheckId::AsyncCopyCancelReader]));
    });
}

/// Missing write cancellation instrumentation is optional by default but can
/// be required explicitly for each stage.
#[test]
fn test_unavailable_write_cancellation_stages_are_optional() {
    use qubit_fs_testkit::ContractCheckOutcome;

    let fixture = AsyncMemoryFixture::without_cancellation_cases();
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(FileSystemContract::Write).await;
        for id in [
            testkit::ContractCheckId::WriteCancelOpen,
            testkit::ContractCheckId::WriteCancelWrite,
            testkit::ContractCheckId::WriteCancelFlush,
            testkit::ContractCheckId::WriteCancelCommit,
        ] {
            let check = run
                .report()
                .checks()
                .iter()
                .find(|check| check.id() == id)
                .expect("write cancellation check must be recorded");
            assert!(matches!(check.outcome(), ContractCheckOutcome::SkippedOptional { reason } if !reason.is_empty()));
        }
        assert!(run.requirements_satisfied());
        assert!(!run.all_applicable_checks_verified());
        assert!(!run.requirements_satisfied_with(&[
            testkit::ContractCheckId::WriteCancelOpen,
            testkit::ContractCheckId::WriteCancelWrite,
            testkit::ContractCheckId::WriteCancelFlush,
            testkit::ContractCheckId::WriteCancelCommit,
        ]));
    });
}

/// One missing stage does not suppress the other three write probes.
#[test]
fn test_single_unavailable_write_cancellation_stage_is_independent() {
    use qubit_fs_testkit::ContractCheckOutcome;

    for missing in [
        testkit::AsyncWriteCancellationStage::Open,
        testkit::AsyncWriteCancellationStage::Write,
        testkit::AsyncWriteCancellationStage::Flush,
        testkit::AsyncWriteCancellationStage::Commit,
    ] {
        let fixture = AsyncMemoryFixture::without_write_cancellation_stage(missing);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_contract(FileSystemContract::Write).await;
            for (id, stage) in [
                (
                    testkit::ContractCheckId::WriteCancelOpen,
                    testkit::AsyncWriteCancellationStage::Open,
                ),
                (
                    testkit::ContractCheckId::WriteCancelWrite,
                    testkit::AsyncWriteCancellationStage::Write,
                ),
                (
                    testkit::ContractCheckId::WriteCancelFlush,
                    testkit::AsyncWriteCancellationStage::Flush,
                ),
                (
                    testkit::ContractCheckId::WriteCancelCommit,
                    testkit::AsyncWriteCancellationStage::Commit,
                ),
            ] {
                let check = run
                    .report()
                    .checks()
                    .iter()
                    .find(|check| check.id() == id)
                    .expect("stage check");
                if stage == missing {
                    assert!(matches!(check.outcome(), ContractCheckOutcome::SkippedOptional { .. }));
                } else {
                    assert!(matches!(check.outcome(), ContractCheckOutcome::Passed));
                }
            }
            assert!(run.requirements_satisfied());
            assert!(!run.requirements_satisfied_with(&[match missing {
                testkit::AsyncWriteCancellationStage::Open => testkit::ContractCheckId::WriteCancelOpen,
                testkit::AsyncWriteCancellationStage::Write => testkit::ContractCheckId::WriteCancelWrite,
                testkit::AsyncWriteCancellationStage::Flush => testkit::ContractCheckId::WriteCancelFlush,
                testkit::AsyncWriteCancellationStage::Commit => testkit::ContractCheckId::WriteCancelCommit,
            }]));
        });
        assert!(fixture.is_empty(), "independent stage probes must be cleaned");
    }
}

/// A false NotPublished abort must be rejected using independent target
/// evidence.
#[test]
fn test_write_cancellation_rejects_abort_that_publishes() {
    let fixture =
        AsyncMemoryFixture::with_fault(self::common::async_memory_file_system::AsyncMemoryFault::WriteAbortPublishes);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            suite.run_contract(FileSystemContract::Write).await.assert_satisfied();
        });
    }));
    run_controlled(fixture.teardown()).expect("explicit cleanup after faulty abort");
    assert!(result.is_err(), "cancellation accepted a false NotPublished claim");
}

/// Preparing a probe must not request a payload that the facade must reject.
#[test]
fn test_write_cancellation_probe_respects_declared_write_limits() {
    use qubit_fs::metadata::FileSystemLimit;
    use qubit_fs::metadata::FileSystemLimits;
    use qubit_fs_testkit::AsyncWriteCancellationStage;
    use qubit_fs_testkit::FixtureSupport;

    for maximum in [1, 4] {
        let fixture = AsyncMemoryFixture::with_cancellation_limits(
            FileSystemLimits::unknown().with_max_write_bytes(FileSystemLimit::Maximum(maximum)),
        );
        for stage in [
            AsyncWriteCancellationStage::Open,
            AsyncWriteCancellationStage::Write,
            AsyncWriteCancellationStage::Flush,
            AsyncWriteCancellationStage::Commit,
        ] {
            let prepared = run_controlled(fixture.prepare_write_cancellation(stage, "bounded-probe"))
                .expect("preparation must succeed");
            match prepared {
                FixtureSupport::Supported(probe) => {
                    assert!(
                        probe.case().bytes().len() as u64 <= maximum,
                        "probe payload exceeds the provider limit"
                    );
                    probe.disarm().expect("prepared probe must disarm");
                }
                FixtureSupport::Unsupported => panic!("positive limits permit every write stage"),
            }
        }
        run_controlled(fixture.teardown()).expect("bounded fixture teardown");
    }
}

/// Cancellation while observing initial state must release the prepared gate.
#[test]
fn test_write_cancellation_disarms_during_initial_observation() {
    let fixture = AsyncMemoryFixture::new();
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let mut assertion = Box::pin(suite.run_contract(FileSystemContract::Write));
    let mut context = Context::from_waker(Waker::noop());
    assert!(assertion.as_mut().poll(&mut context).is_pending());
    assert!(fixture.write_cancellation_is_armed());
    assert!(fixture.write_cancellation_stages().is_empty());
    drop(assertion);
    assert!(
        !fixture.write_cancellation_is_armed(),
        "observation cancellation leaked the gate"
    );
    run_controlled(fixture.teardown()).expect("explicit observation cleanup");
}

/// Missing copy capability makes cancellation inapplicable, not unverified.
#[test]
fn test_unavailable_copy_does_not_require_cancellation_probes() {
    let fixture = AsyncMemoryFixture::without_core_capabilities();
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(testkit::FileSystemContract::Copy).await;
        assert!(run.all_applicable_checks_verified());
    });
}

/// An executed optional probe failure invalidates the entire run.
#[test]
fn test_failed_optional_write_probe_invalidates_run() {
    use qubit_fs_testkit::ContractCheckId;
    use qubit_fs_testkit::ContractCheckOutcome;
    use qubit_fs_testkit::FileSystemContract;

    let fixture =
        AsyncMemoryFixture::with_fault(self::common::async_memory_file_system::AsyncMemoryFault::WriteAbortPublishes);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(FileSystemContract::Write).await;
        assert!(!run.requirements_satisfied());
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::WriteCancelWrite));
        let failed = run
            .report()
            .checks()
            .iter()
            .find(|check| check.id() == ContractCheckId::WriteCancelWrite)
            .expect("registered optional probe");
        assert!(matches!(failed.outcome(), ContractCheckOutcome::Failed { .. }));
        assert!(
            run.report()
                .checks()
                .iter()
                .any(|check| check.id() == ContractCheckId::WriteCancelFlush
                    && matches!(check.outcome(), ContractCheckOutcome::NotRun { .. }))
        );
        assert!(fixture.is_empty(), "failure must still invoke independent teardown");
    });
}

/// Copy cancellation must verify an abort's claim against the actual target.
#[test]
fn test_failed_optional_copy_probe_invalidates_run() {
    use qubit_fs_testkit::ContractCheckId;
    use qubit_fs_testkit::ContractCheckOutcome;
    use qubit_fs_testkit::FileSystemContract;

    let fixture =
        AsyncMemoryFixture::with_fault(self::common::async_memory_file_system::AsyncMemoryFault::CopyAbortPublishes);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(FileSystemContract::Copy).await;
        assert!(!run.requirements_satisfied(), "copy probe accepted false NotPublished");
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::AsyncCopyCancelWriter));
        assert!(
            run.report()
                .checks()
                .iter()
                .any(|check| check.id() == ContractCheckId::AsyncCopyCancelWriter
                    && matches!(check.outcome(), ContractCheckOutcome::Failed { .. }))
        );
        assert!(fixture.is_empty(), "failed copy must still teardown");
    });
}
