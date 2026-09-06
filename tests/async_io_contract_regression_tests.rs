#![cfg(feature = "async")]

mod common;

use common::async_memory_file_system::run_controlled;
use common::{AsyncMemoryFault, AsyncMemoryFixture};
use std::task::Poll;
use qubit_fs::metadata::{FileSystemLimit, FileSystemLimits};
use qubit_fs_testkit::{AsyncFileSystemContractSuite, FileSystemContract, FixtureCase};

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
    run_controlled(
        AsyncFileSystemContractSuite::new(&fixture).assert_contract(FileSystemContract::Write),
    );
}

#[test]
fn async_write_limit_zero_and_small_boundaries_are_reported() {
    for limit in [
        FileSystemLimit::Maximum(0),
        FileSystemLimit::Maximum(1),
        FileSystemLimit::Maximum(4),
        FileSystemLimit::Unknown,
        FileSystemLimit::NotApplicable,
        FileSystemLimit::Unbounded,
        FileSystemLimit::Maximum(u64::MAX),
    ] {
        let fixture = AsyncMemoryFixture::with_limits(
            FileSystemLimits::unknown().with_max_write_bytes(limit),
        );
        let report = run_controlled(
            AsyncFileSystemContractSuite::new(&fixture)
                .assert_contract_with_report(FileSystemContract::Write),
        );
        report.assert_complete();
    }
}

#[test]
fn conditional_case_unavailability_is_reported_as_unverified() {
    let fixture = AsyncMemoryFixture::with_conditional_case_unavailable(FixtureCase::ReadIfMatch);
    let report = run_controlled(
        AsyncFileSystemContractSuite::new(&fixture)
            .assert_contract_with_report(FileSystemContract::Read),
    );
    assert!(report.checks().iter().any(|check| {
        check.id() == "read/if-match-current"
            && matches!(
                check.outcome(),
                qubit_fs_testkit::ContractCheckOutcome::Unverified { .. }
            )
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
        assert!(
            result.is_err(),
            "fault was not caught by async suite: {fault:?}"
        );
    }
}
