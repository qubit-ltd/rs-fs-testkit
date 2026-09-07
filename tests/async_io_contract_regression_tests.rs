// qubit-style: allow explicit-imports
#![cfg(feature = "async")]

mod common;
use std::task::Poll;

use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::AsyncWriteCancellationStage;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContract;

use self::common::AsyncMemoryFault;
use self::common::AsyncMemoryFixture;
use self::common::async_memory_file_system::run_controlled;
use crate::common::UnavailableScenario;
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
    run_controlled(async {
        AsyncFileSystemContractSuite::new(&fixture)
            .run_contract(FileSystemContract::Write)
            .await
            .assert_satisfied()
    });
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
        let report = run_controlled(async {
            AsyncFileSystemContractSuite::new(&fixture)
                .run_contract(FileSystemContract::Write)
                .await
                .report()
                .clone()
        });
        report.assert_satisfied();
    }
}

#[test]
fn async_property_limit_boundaries_are_reported() {
    let limits = FileSystemLimits::unknown()
        .with_max_component_text_bytes(FileSystemLimit::Maximum(4))
        .with_max_list_page_entries(FileSystemLimit::Maximum(2));
    let fixture = AsyncMemoryFixture::with_limits(limits);
    let report = run_controlled(async {
        AsyncFileSystemContractSuite::new(&fixture)
            .run_contract(FileSystemContract::Properties)
            .await
            .report()
            .clone()
    });
    report.assert_satisfied();
}

#[test]
fn conditional_case_unavailability_is_reported_as_unverified() {
    let fixture = AsyncMemoryFixture::with_conditional_case_unavailable(UnavailableScenario::ReadIfMatch);
    let report = run_controlled(async {
        AsyncFileSystemContractSuite::new(&fixture)
            .run_contract(FileSystemContract::Read)
            .await
            .report()
            .clone()
    });
    assert!(report.checks().iter().any(|check| {
        check.id().as_str() == "read/if-match-current"
            && matches!(check.outcome(), ContractCheckOutcome::Unverified { .. })
    }));
}

#[test]
fn async_conditional_delete_case_unavailability_is_unverified() {
    let fixture = AsyncMemoryFixture::with_conditional_case_unavailable(UnavailableScenario::DeleteIfMatch);
    let report = run_controlled(async {
        AsyncFileSystemContractSuite::new(&fixture)
            .run_contract(FileSystemContract::Delete)
            .await
            .report()
            .clone()
    });
    assert!(report.checks().iter().any(|check| {
        check.id().as_str() == "delete/if-match" && matches!(check.outcome(), ContractCheckOutcome::Unverified { .. })
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
            run_controlled(async {
                AsyncFileSystemContractSuite::new(&fixture)
                    .run_all()
                    .await
                    .assert_satisfied()
            });
        }));
        assert!(result.is_err(), "fault was not caught by async suite: {fault:?}");
    }
}

/// A deliberately invalid request must be observed by the real facade call;
/// recording RejectedAsExpected without issuing it cannot pass this regression.
#[test]
fn async_negative_write_guarantees_issue_real_requests() {
    let fixture = AsyncMemoryFixture::without_optional_capabilities();
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        suite.run_contract(FileSystemContract::Write).await.assert_satisfied();
    });
    for suffix in ["async-atomic-replace-unavailable", "async-durable-write"] {
        let fixture = AsyncMemoryFixture::without_optional_capabilities().with_invalid_probe_path(suffix);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                suite.run_contract(FileSystemContract::Write).await.assert_satisfied();
            })
        }));
        assert!(result.is_err(), "{suffix}: the incompatible request was never checked");
        use qubit_fs_testkit::AsyncFileSystemFixture;
        run_controlled(fixture.teardown()).expect("independent teardown");
    }
}
