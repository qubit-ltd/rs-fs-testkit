#![cfg(feature = "async")]

mod common;

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use common::{AsyncMemoryFault, AsyncMemoryFixture};
use qubit_fs::metadata::{FileSystemLimit, FileSystemLimits};
use qubit_fs_testkit::{AsyncFileSystemContractSuite, FileSystemContract, FixtureCase};

struct WakeFlag(AtomicBool);

impl Wake for WakeFlag {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::Release);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.store(true, Ordering::Release);
    }
}

fn run_controlled<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    let flag = Arc::new(WakeFlag(AtomicBool::new(true)));
    let waker = Waker::from(Arc::clone(&flag));
    let mut context = Context::from_waker(&waker);
    for _ in 0..1024 {
        flag.0.store(false, Ordering::Release);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => assert!(
                flag.0.load(Ordering::Acquire),
                "controlled future returned Pending without scheduling a wake"
            ),
        }
    }
    panic!("controlled future exceeded the poll budget")
}

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
fn async_limits_allow_single_byte_write() {
    let limits = FileSystemLimits::unknown().with_max_write_bytes(FileSystemLimit::Maximum(1));
    let fixture = AsyncMemoryFixture::with_limits(limits);
    run_controlled(
        AsyncFileSystemContractSuite::new(&fixture).assert_contract(FileSystemContract::Write),
    );
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
