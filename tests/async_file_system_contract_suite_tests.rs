// =============================================================================

#![cfg(feature = "async")]
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use std::future::Future;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use common::AsyncMemoryFault;
use common::AsyncMemoryFixture;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::AsyncFileSystemFixture;

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
        FileSystemCapability::DurableFileCopy,
        FileSystemCapability::AtomicTempPersist,
    ] {
        assert!(
            capabilities.supports(capability),
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

/// Every advertised capability executes its positive asynchronous contract.
#[test]
fn test_all_capabilities_execute_async_contracts() {
    let fixture = AsyncMemoryFixture::with_all_capabilities();
    assert_eq!(
        fixture
            .file_system()
            .properties()
            .capabilities()
            .iter()
            .collect::<Vec<_>>(),
        FileSystemCapability::ALL.to_vec()
    );
    let mut assertion =
        Box::pin(AsyncFileSystemContractSuite::new(&fixture).assert_all());
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(
        assertion.as_mut().poll(&mut context),
        Poll::Ready(())
    ));
    assert!(fixture.is_empty(), "all-capability suite must clean up");
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

/// Copy cancellation is independently callable for fixtures with probes.
#[test]
fn test_async_copy_cancellation_contract_is_independently_executable() {
    let fixture = AsyncMemoryFixture::new();
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let mut assertion = Box::pin(async {
        suite.assert_copy_cancellation().await;
        suite.finish().await;
    });
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(
        assertion.as_mut().poll(&mut context),
        Poll::Ready(())
    ));
    assert!(
        fixture.is_empty(),
        "cancellation contract must clean resources"
    );
}

/// A successful provider-native copy is a valid advertised Copy implementation.
#[test]
fn test_async_copy_accepts_native_outcome() {
    let fixture = AsyncMemoryFixture::with_native_copy();
    assert_copy_contract(&fixture);
}

/// Object and prefix metadata kinds satisfy asynchronous provider-neutral
/// contracts and cleanup.
#[test]
fn test_async_suite_accepts_object_and_prefix_kinds() {
    let fixture = AsyncMemoryFixture::with_object_kinds();
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let mut assertion = Box::pin(async {
        suite.assert_stat().await;
        suite.assert_create_directory().await;
        suite.finish().await;
    });
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(
        assertion.as_mut().poll(&mut context),
        Poll::Ready(())
    ));
    assert!(fixture.is_empty(), "object resources must be cleaned");
}

/// Recursive prefix deletion does not imply asynchronous directory creation.
#[test]
fn test_async_recursive_delete_does_not_require_create_directory() {
    let fixture =
        AsyncMemoryFixture::recursive_delete_without_create_directory();
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let mut assertion = Box::pin(async {
        suite.assert_recursive_delete().await;
        suite.finish().await;
    });
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(
        assertion.as_mut().poll(&mut context),
        Poll::Ready(())
    ));
    assert!(
        fixture.is_empty(),
        "recursive deletion must remove the prefix"
    );
}

/// An asynchronous assertion panic is resumed only after cleanup completes.
#[test]
fn test_async_suite_cleans_resources_before_resuming_panic() {
    let fixture =
        AsyncMemoryFixture::with_fault(AsyncMemoryFault::WriteDropsBytes);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut assertion =
            Box::pin(AsyncFileSystemContractSuite::new(&fixture).assert_all());
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let _ = assertion.as_mut().poll(&mut context);
    }));
    assert!(result.is_err(), "injected write fault must fail the suite");
    assert!(
        fixture.is_empty(),
        "failed suite must clean published paths"
    );
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
        9,
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
        suite.assert_durable_rename().await;
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
        AsyncMemoryFault::WrongStatMetadata,
        AsyncMemoryFault::ReadWrongBytes,
        AsyncMemoryFault::WriteDropsBytes,
        AsyncMemoryFault::ListEscapesNamespace,
        AsyncMemoryFault::EmptyList,
        AsyncMemoryFault::ListDropsMetadata,
        AsyncMemoryFault::DeleteNoOp,
        AsyncMemoryFault::CopyDropsTarget,
        AsyncMemoryFault::RenameNoOp,
        AsyncMemoryFault::RenameWrongOutcome,
        AsyncMemoryFault::DirectoryCopyDropsChildren,
        AsyncMemoryFault::TempCleanupNoOp,
        AsyncMemoryFault::AppendOverwrites,
        AsyncMemoryFault::RecursiveDeleteLeavesChildren,
        AsyncMemoryFault::AtomicRenameNonAtomic,
        AsyncMemoryFault::AtomicReplaceNonAtomic,
        AsyncMemoryFault::DurableFileCopyNonDurable,
        AsyncMemoryFault::DurableRenameNonDurable,
        AsyncMemoryFault::TempPersistWrongTarget,
        AsyncMemoryFault::AtomicTempPersistNonAtomic,
        AsyncMemoryFault::TempIgnoresOptions,
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
