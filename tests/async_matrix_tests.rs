// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================

mod common;

use std::future::Future;
use std::task::{Context, Poll, Waker};

use common::{AsyncMemoryFault, AsyncMemoryFixture};
use qubit_fs_testkit::AsyncFileSystemContractSuite;

/// A conforming asynchronous provider must satisfy every suite phase.
#[test]
fn test_conforming_async_memory_provider_satisfies_full_suite() {
    let fixture = AsyncMemoryFixture::new();
    let mut assertion = Box::pin(AsyncFileSystemContractSuite::new(&fixture).assert_all());
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(
        assertion.as_mut().poll(&mut context),
        Poll::Ready(())
    ));
    assert!(fixture.is_empty(), "suite must clean up created resources");
}

/// Each isolated asynchronous provider fault must fail the full suite.
#[test]
fn test_single_faults_are_rejected_by_async_suite() {
    for fault in [
        AsyncMemoryFault::MissingPathExists,
        AsyncMemoryFault::ReadWrongBytes,
        AsyncMemoryFault::WriteDropsBytes,
        AsyncMemoryFault::ListEscapesNamespace,
        AsyncMemoryFault::DeleteNoOp,
        AsyncMemoryFault::CopyDropsTarget,
        AsyncMemoryFault::RenameNoOp,
        AsyncMemoryFault::TempCleanupNoOp,
    ] {
        let fixture = AsyncMemoryFixture::with_fault(fault);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut assertion = Box::pin(AsyncFileSystemContractSuite::new(&fixture).assert_all());
            let waker = Waker::noop();
            let mut context = Context::from_waker(waker);
            assert!(matches!(
                assertion.as_mut().poll(&mut context),
                Poll::Ready(())
            ));
        }));
        assert!(
            result.is_err(),
            "suite accepted injected async fault: {fault:?}"
        );
    }
}
