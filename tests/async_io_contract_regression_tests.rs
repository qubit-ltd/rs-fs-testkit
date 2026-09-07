// qubit-style: allow explicit-imports
#![cfg(feature = "async")]

mod common;
#[path = "common/panic_support.rs"]
mod panic_support;

use std::task::Poll;

use common::AsyncMemoryFault;
use common::AsyncMemoryFixture;
use common::async_memory_file_system::run_controlled;
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::AsyncWriteCancellationStage;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FixtureCase;

#[test]
fn controlled_runner_accepts_real_pending_then_completion() {
    let mut first = true;
    let future = std::future::poll_fn(move |context| {
        if first {
            first = false;
            context.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(7_u8)
        }
    });
    assert_eq!(run_controlled(future), 7);
}

#[test]
fn controlled_runner_does_not_treat_first_pending_as_completion() {
    let mut remaining = 2_u8;
    let future = std::future::poll_fn(move |context| {
        if remaining != 0 {
            remaining -= 1;
            context.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(11_u8)
        }
    });
    assert_eq!(run_controlled(future), 11);
}

#[test]
fn async_limits_allow_single_byte_write() {
    let limits = FileSystemLimits::unknown().with_max_write_bytes(FileSystemLimit::Maximum(1));
    let fixture = AsyncMemoryFixture::with_limits(limits);
    run_controlled(AsyncFileSystemContractSuite::new(&fixture).assert_contract(FileSystemContract::Write));
}

#[test]
fn async_write_limit_small_boundaries_are_reported() {
    for limit in [
        FileSystemLimit::Maximum(1),
        FileSystemLimit::Maximum(4),
        FileSystemLimit::Unknown,
        FileSystemLimit::NotApplicable,
        FileSystemLimit::Unbounded,
        FileSystemLimit::Maximum(u64::MAX),
    ] {
        let fixture = AsyncMemoryFixture::with_limits(FileSystemLimits::unknown().with_max_write_bytes(limit));
        let report = run_controlled(
            AsyncFileSystemContractSuite::new(&fixture).assert_contract_with_report(FileSystemContract::Write),
        );
        report.assert_complete();
    }
}

#[test]
fn async_property_limit_boundaries_are_reported() {
    let limits = FileSystemLimits::unknown()
        .with_max_component_text_bytes(FileSystemLimit::Maximum(4))
        .with_max_list_page_entries(FileSystemLimit::Maximum(2));
    let fixture = AsyncMemoryFixture::with_limits(limits);
    let report = run_controlled(
        AsyncFileSystemContractSuite::new(&fixture).assert_contract_with_report(FileSystemContract::Properties),
    );
    report.assert_complete();
}

#[test]
fn conditional_case_unavailability_is_reported_as_unverified() {
    let fixture = AsyncMemoryFixture::with_conditional_case_unavailable(FixtureCase::ReadIfMatch);
    let report = run_controlled(
        AsyncFileSystemContractSuite::new(&fixture).assert_contract_with_report(FileSystemContract::Read),
    );
    assert!(report.checks().iter().any(|check| {
        check.id() == "read/if-match-current" && matches!(check.outcome(), ContractCheckOutcome::Unverified { .. })
    }));
}

#[test]
fn async_conditional_delete_case_unavailability_is_unverified() {
    let fixture = AsyncMemoryFixture::with_conditional_case_unavailable(FixtureCase::DeleteIfMatch);
    let report = run_controlled(
        AsyncFileSystemContractSuite::new(&fixture).assert_contract_with_report(FileSystemContract::Delete),
    );
    assert!(report.checks().iter().any(|check| {
        check.id() == "delete/if-match" && matches!(check.outcome(), ContractCheckOutcome::Unverified { .. })
    }));
}

#[test]
fn async_fault_profiles_are_exercised_by_the_real_suite() {
    for fault in [
        AsyncMemoryFault::IgnoreReadIfMatch,
        AsyncMemoryFault::IgnoreReadIfNoneMatch,
        AsyncMemoryFault::IgnoreWriteIfMatch,
        AsyncMemoryFault::AtomicReplaceKeepsOldBytes,
        AsyncMemoryFault::DurableWriteDropsBytes,
        AsyncMemoryFault::ChecksumIgnoresCorruption,
    ] {
        let fixture = AsyncMemoryFixture::with_fault(fault);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_controlled(AsyncFileSystemContractSuite::new(&fixture).assert_all());
        }));
        assert!(result.is_err(), "fault was not caught by async suite: {fault:?}");
    }
}

#[test]
fn write_cancellation_requires_real_stage_evidence() {
    let fixture = AsyncMemoryFixture::without_cancellation_cases();
    let report = run_controlled(
        AsyncFileSystemContractSuite::new(&fixture).assert_contract_with_report(FileSystemContract::Write),
    );
    for id in [
        "write/cancel-open",
        "write/cancel-write",
        "write/cancel-flush",
        "write/cancel-commit",
    ] {
        assert!(
            report
                .checks()
                .iter()
                .any(|check| check.id() == id && matches!(check.outcome(), ContractCheckOutcome::Unverified { .. })),
            "missing unverified check: {id}"
        );
    }
    assert!(!report.is_complete());
}

#[test]
fn write_cancellation_observes_all_four_provider_stages() {
    let fixture = AsyncMemoryFixture::new();
    let report = run_controlled(
        AsyncFileSystemContractSuite::new(&fixture).assert_contract_with_report(FileSystemContract::Write),
    );
    for id in [
        "write/owning-operation",
        "write/repeated-execute",
        "write/cancel-open",
        "write/cancel-write",
        "write/cancel-flush",
        "write/cancel-commit",
    ] {
        assert!(
            report
                .checks()
                .iter()
                .any(|check| check.id() == id && check.outcome() == &ContractCheckOutcome::Passed),
            "missing evidence: {id}"
        );
    }
    report.assert_complete();
    assert_eq!(
        vec![
            AsyncWriteCancellationStage::Open,
            AsyncWriteCancellationStage::Write,
            AsyncWriteCancellationStage::Flush,
            AsyncWriteCancellationStage::Commit
        ],
        fixture.prepared_write_stages()
    );
    assert!(fixture.is_empty());
}

#[test]
fn one_missing_write_stage_prevents_strict_completion() {
    let fixture = AsyncMemoryFixture::without_write_stage(AsyncWriteCancellationStage::Flush);
    let report = run_controlled(
        AsyncFileSystemContractSuite::new(&fixture).assert_contract_with_report(FileSystemContract::Write),
    );
    let missing: Vec<_> = report
        .checks()
        .iter()
        .filter(|check| matches!(check.outcome(), ContractCheckOutcome::Unverified { .. }))
        .map(|check| check.id())
        .collect();
    assert_eq!(vec!["write/cancel-flush"], missing);
    assert!(std::panic::catch_unwind(|| report.assert_complete()).is_err());
    assert_eq!(3, fixture.prepared_write_stages().len());
    assert!(fixture.is_empty());
}

/// Assertion and disarm failures remain diagnosable after fixture cleanup.
#[test]
fn write_probe_failure_disarms_and_preserves_both_diagnostics() {
    for fail_disarm in [false, true] {
        let fixture = AsyncMemoryFixture::with_write_probe_failures(fail_disarm);
        let message = panic_support::catch_message(std::panic::AssertUnwindSafe(|| {
            run_controlled(AsyncFileSystemContractSuite::new(&fixture).assert_contract(FileSystemContract::Write));
        }));
        assert!(
            message.contains("injected write acknowledgement failure"),
            "primary diagnostic lost: {message}"
        );
        if fail_disarm {
            assert!(
                message.contains("injected write disarm failure"),
                "disarm diagnostic lost: {message}"
            );
        }
        assert!(!fixture.write_gate_is_armed());
        assert!(fixture.is_empty());
    }
}
