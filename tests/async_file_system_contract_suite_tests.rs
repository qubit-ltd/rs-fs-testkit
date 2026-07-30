// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use std::future::Future;
use std::task::{
    Context,
    Poll,
    Waker,
};

use common::{
    AsyncMemoryFault,
    AsyncMemoryFixture,
};
use qubit_fs::FileSystemCapability;
use qubit_fs_testkit::{
    AsyncFileSystemContractSuite,
    AsyncFileSystemFixture,
};

/// Polls one copy contract that is expected to complete without suspension.
fn assert_copy_contract(fixture: &AsyncMemoryFixture) {
    let mut suite = AsyncFileSystemContractSuite::new(fixture);
    let mut assertion = Box::pin(suite.assert_copy());
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(
        assertion.as_mut().poll(&mut context),
        Poll::Ready(())
    ));
}

/// A conforming asynchronous provider satisfies every suite phase.
#[test]
fn test_conforming_async_memory_provider_satisfies_full_suite() {
    let fixture = AsyncMemoryFixture::new();
    let capabilities = fixture.file_system().properties().capabilities();
    for capability in [
        FileSystemCapability::Append,
        FileSystemCapability::RecursiveDelete,
        FileSystemCapability::AtomicRename,
        FileSystemCapability::AtomicReplace,
        FileSystemCapability::DurableCopy,
        FileSystemCapability::AtomicTempPersist,
    ] {
        assert!(
            capabilities.contains(capability),
            "conforming async fixture must exercise {capability:?}"
        );
    }
    let mut assertion =
        Box::pin(AsyncFileSystemContractSuite::new(&fixture).assert_all());
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(
        assertion.as_mut().poll(&mut context),
        Poll::Ready(())
    ));
    assert!(fixture.is_empty(), "suite must clean up created resources");
}

/// An asynchronous filesystem may use its own identifier as the provider
/// identifier.
#[test]
fn test_async_suite_allows_matching_filesystem_and_provider_ids() {
    let fixture = AsyncMemoryFixture::with_matching_ids();
    let mut assertion =
        Box::pin(AsyncFileSystemContractSuite::new(&fixture).assert_all());
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(
        assertion.as_mut().poll(&mut context),
        Poll::Ready(())
    ));
}

/// Copy cancellation cases are optional fixture probes, not provider
/// requirements.
#[test]
fn test_async_copy_allows_fixture_without_cancellation_cases() {
    let fixture = AsyncMemoryFixture::without_cancellation_cases();
    assert_copy_contract(&fixture);
}

/// A successful provider-native copy is a valid advertised Copy implementation.
#[test]
fn test_async_copy_accepts_native_outcome() {
    let fixture = AsyncMemoryFixture::with_native_copy();
    assert_copy_contract(&fixture);
}

/// Unadvertised async core operations still exercise facade preflight errors.
#[test]
fn test_async_core_capability_negative_branches_are_exercised() {
    let fixture = AsyncMemoryFixture::without_operation_capabilities();
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let mut assertion = Box::pin(async {
        suite.assert_read().await;
        suite.assert_write().await;
        suite.assert_list().await;
        suite.assert_create_directory().await;
        suite.assert_delete().await;
        suite.assert_copy().await;
        suite.assert_rename().await;
    });
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(
        assertion.as_mut().poll(&mut context),
        Poll::Ready(())
    ));
    assert_eq!(
        fixture.path_call_count(),
        8,
        "negative capability branches must exercise their facade paths"
    );
}

/// Unadvertised optional operations return structured errors while core
/// contracts continue to exercise the provider.
#[test]
fn test_async_suite_skips_unadvertised_optional_capabilities() {
    let fixture = AsyncMemoryFixture::without_optional_capabilities();
    let mut assertion =
        Box::pin(AsyncFileSystemContractSuite::new(&fixture).assert_all());
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(
        assertion.as_mut().poll(&mut context),
        Poll::Ready(())
    ));
}

/// Every public asynchronous contract phase remains independently pollable for
/// providers that execute without suspension.
#[test]
fn test_async_contract_entry_points_run_individually() {
    let fixture = AsyncMemoryFixture::new();
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let mut assertion = Box::pin(async {
        suite.assert_properties().await;
        suite.assert_stat().await;
        suite.assert_read().await;
        suite.assert_write().await;
        suite.assert_list().await;
        suite.assert_create_directory().await;
        suite.assert_delete().await;
        suite.assert_copy().await;
        suite.assert_rename().await;
        suite.assert_append().await;
        suite.assert_recursive_delete().await;
        suite.assert_atomic_rename().await;
        suite.assert_atomic_replace().await;
        suite.assert_durable_copy().await;
        suite.assert_temp_resources().await;
        suite.assert_error_context().await;
    });
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(
        assertion.as_mut().poll(&mut context),
        Poll::Ready(())
    ));
}

/// Each isolated asynchronous provider fault must fail the full suite.
#[test]
fn test_single_faults_are_rejected_by_async_suite() {
    for fault in [
        AsyncMemoryFault::MissingPathExists,
        AsyncMemoryFault::ReadWrongBytes,
        AsyncMemoryFault::WriteDropsBytes,
        AsyncMemoryFault::ListEscapesNamespace,
        AsyncMemoryFault::EmptyList,
        AsyncMemoryFault::DeleteNoOp,
        AsyncMemoryFault::CopyDropsTarget,
        AsyncMemoryFault::RenameNoOp,
        AsyncMemoryFault::TempCleanupNoOp,
        AsyncMemoryFault::AppendOverwrites,
        AsyncMemoryFault::RecursiveDeleteLeavesChildren,
        AsyncMemoryFault::AtomicRenameNonAtomic,
        AsyncMemoryFault::AtomicReplaceNonAtomic,
        AsyncMemoryFault::DurableCopyNonDurable,
        AsyncMemoryFault::TempPersistWrongTarget,
        AsyncMemoryFault::AtomicTempPersistNonAtomic,
    ] {
        let fixture = AsyncMemoryFixture::with_fault(fault);
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut assertion = Box::pin(
                    AsyncFileSystemContractSuite::new(&fixture).assert_all(),
                );
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
