// qubit-style: allow explicit-imports
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

use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs_testkit as testkit;
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContract;

use self::common::AsyncMemoryFault;
use self::common::AsyncMemoryFixture;
use self::common::async_memory_file_system::run_controlled;
use self::common::check_matrix::assert_panics_at;
use self::common::check_matrix::async_fault_cases;
use crate::common::UnavailableScenario;
/// Polls one copy contract that is expected to complete without suspension.
fn assert_copy_contract(fixture: &AsyncMemoryFixture) {
    let mut suite = AsyncFileSystemContractSuite::new(fixture);
    let mut assertion = Box::pin(async { suite.run_contract(FileSystemContract::Copy).await.assert_satisfied() });
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(assertion.as_mut().poll(&mut context), Poll::Ready(())));
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
    run_controlled(async {
        AsyncFileSystemContractSuite::new(&fixture)
            .run_all()
            .await
            .assert_satisfied()
    });
    assert!(fixture.is_empty(), "suite must clean up created resources");
}

#[test]
fn test_async_phase_matrix_exercises_declared_profiles() {
    for contract in FileSystemContract::ALL {
        for profile in 0_u8..4 {
            let fixture = match profile {
                0 => AsyncMemoryFixture::with_all_capabilities(),
                1 => AsyncMemoryFixture::without_operation_capabilities(),
                2 => AsyncMemoryFixture::without_optional_capabilities(),
                _ => AsyncMemoryFixture::fallback_only(),
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_controlled(async {
                    AsyncFileSystemContractSuite::new(&fixture)
                        .run_contract(contract)
                        .await
                        .assert_satisfied()
                });
            }));
            assert!(result.is_ok(), "async profile {profile} panicked in {contract:?}");
        }
    }
}

/// A fallback-only provider rejects native-only conflict and tree requests.
#[test]
fn test_async_fallback_copy_rejects_native_conflicts() {
    let fixture = AsyncMemoryFixture::fallback_only();
    let report = run_controlled(async {
        AsyncFileSystemContractSuite::new(&fixture)
            .run_contract(FileSystemContract::Copy)
            .await
            .report()
            .clone()
    });
    report.assert_satisfied();
    assert!(report.checks().iter().any(|check| {
        check.id().as_str() == "copy/fallback-overwrite-rejected"
            && matches!(check.outcome(), ContractCheckOutcome::RejectedAsExpected)
    }));
    assert!(report.checks().iter().any(|check| {
        check.id().as_str() == "copy/atomic-tree" && matches!(check.outcome(), ContractCheckOutcome::RejectedAsExpected)
    }));
    assert!(fixture.is_empty(), "fallback copy contract leaked resources");
}

#[test]
fn test_async_faults_exercise_full_suite_paths() {
    for case in async_fault_cases() {
        for contract in FileSystemContract::ALL {
            let fixture = AsyncMemoryFixture::with_fault(case.fault);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_controlled(async {
                    AsyncFileSystemContractSuite::new(&fixture)
                        .run_contract(contract)
                        .await
                        .assert_satisfied()
                });
            }));
        }
    }
}

#[test]
fn test_async_property_profiles_cover_all_limit_outcomes() {
    let limits = [
        FileSystemLimit::Maximum(0),
        FileSystemLimit::Maximum(4),
        FileSystemLimit::Unknown,
        FileSystemLimit::Unbounded,
        FileSystemLimit::NotApplicable,
        FileSystemLimit::Maximum(u64::MAX),
    ];
    for limit in limits {
        let snapshot = FileSystemLimits::unknown()
            .with_max_path_text_bytes(limit)
            .with_max_component_text_bytes(limit)
            .with_max_list_page_entries(limit);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let fixture = AsyncMemoryFixture::with_limits(snapshot);
            run_controlled(async {
                AsyncFileSystemContractSuite::new(&fixture)
                    .run_contract(FileSystemContract::Properties)
                    .await
                    .assert_satisfied()
            });
        }));
    }
}

#[test]
fn test_async_unavailable_fixture_cases_are_exercised() {
    let cases = [
        UnavailableScenario::ReadIfMatch,
        UnavailableScenario::ReadIfNoneMatch,
        UnavailableScenario::WriteIfAbsent,
        UnavailableScenario::WriteIfMatch,
        UnavailableScenario::DeleteIfMatch,
        UnavailableScenario::CopyOverwrite,
        UnavailableScenario::CopyTree,
        UnavailableScenario::Capability(FileSystemCapability::ServerSideCopy),
        UnavailableScenario::Capability(FileSystemCapability::AtomicFileCopy),
        UnavailableScenario::Capability(FileSystemCapability::AtomicTreeCopy),
        UnavailableScenario::Capability(FileSystemCapability::DurableFileCopy),
        UnavailableScenario::Capability(FileSystemCapability::DurableTreeCopy),
    ];
    for case in cases {
        let fixture = AsyncMemoryFixture::with_conditional_case_unavailable(case);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_controlled(async {
                AsyncFileSystemContractSuite::new(&fixture)
                    .run_all()
                    .await
                    .assert_satisfied()
            });
        }));
    }
}

#[test]
fn test_async_cleanup_retains_resources_for_failures() {
    for fault in [
        AsyncMemoryFault::CleanupDeleteError,
        AsyncMemoryFault::CleanupDeletePanic,
    ] {
        let fixture = AsyncMemoryFixture::with_fault(fault);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_controlled(async {
                AsyncFileSystemContractSuite::new(&fixture)
                    .run_contract(FileSystemContract::Write)
                    .await
                    .assert_satisfied()
            });
        }));
        assert!(result.is_err(), "async cleanup fault must be reported: {fault:?}");
    }
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
    run_controlled(async {
        AsyncFileSystemContractSuite::new(&fixture)
            .run_all()
            .await
            .assert_satisfied()
    });
    assert!(fixture.is_empty(), "all-capability suite must clean up");
}

/// An asynchronous filesystem may use its own identifier as the provider
/// identifier.
#[test]
fn test_async_suite_allows_matching_filesystem_and_provider_ids() {
    let fixture = AsyncMemoryFixture::with_matching_ids();
    run_controlled(async {
        AsyncFileSystemContractSuite::new(&fixture)
            .run_all()
            .await
            .assert_satisfied()
    });
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
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        suite.run_contract(FileSystemContract::Copy).await.assert_satisfied();
    });
    assert!(fixture.is_empty(), "cancellation contract must clean resources");
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
    run_controlled(async {
        for phase in [FileSystemContract::Stat, FileSystemContract::CreateDirectory] {
            let fixture = AsyncMemoryFixture::with_object_kinds();
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            suite.run_contract(phase).await.assert_satisfied();
            assert!(fixture.is_empty(), "object resources must be cleaned");
        }
    });
}

/// Recursive prefix deletion does not imply asynchronous directory creation.
#[test]
fn test_async_recursive_delete_does_not_require_create_directory() {
    let fixture = AsyncMemoryFixture::recursive_delete_without_create_directory();
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let mut assertion = Box::pin(async {
        suite
            .run_check(testkit::ContractCheckId::DeleteTree)
            .await
            .assert_satisfied();
    });
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(assertion.as_mut().poll(&mut context), Poll::Ready(())));
    assert!(fixture.is_empty(), "recursive deletion must remove the prefix");
}

/// An asynchronous assertion panic is resumed only after cleanup completes.
#[test]
fn test_async_suite_cleans_resources_before_resuming_panic() {
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::WriteDropsBytes);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut assertion = Box::pin(async {
            AsyncFileSystemContractSuite::new(&fixture)
                .run_all()
                .await
                .assert_satisfied()
        });
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let _ = assertion.as_mut().poll(&mut context);
    }));
    assert!(result.is_err(), "injected write fault must fail the suite");
    assert!(fixture.is_empty(), "failed suite must clean published paths");
}

/// Unadvertised async core operations still exercise facade preflight errors.
#[test]
fn test_async_core_capability_negative_branches_are_exercised() {
    run_controlled(async {
        for id in [
            testkit::ContractCheckId::ReadBasic,
            testkit::ContractCheckId::WriteBasic,
            testkit::ContractCheckId::ListBasic,
            testkit::ContractCheckId::DirectoryCreate,
            testkit::ContractCheckId::DirectoryRecursive,
            testkit::ContractCheckId::DeleteBasic,
            testkit::ContractCheckId::CopyBasic,
            testkit::ContractCheckId::RenameBasic,
        ] {
            let fixture = AsyncMemoryFixture::without_operation_capabilities();
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_contract(id.contract()).await;
            run.assert_satisfied();
            let check = run
                .report()
                .checks()
                .iter()
                .find(|check| check.id() == id)
                .unwrap_or_else(|| panic!("{id}: core negative check must execute"));
            assert!(
                matches!(check.outcome(), testkit::ContractCheckOutcome::RejectedAsExpected),
                "{id}: expected an actual facade rejection, got {:?}",
                check.outcome()
            );
        }
    });
}

/// Unadvertised optional operations return structured errors while core
/// contracts continue to exercise the provider.
#[test]
fn test_async_suite_skips_unadvertised_optional_capabilities() {
    let fixture = AsyncMemoryFixture::without_optional_capabilities();
    run_controlled(async {
        AsyncFileSystemContractSuite::new(&fixture)
            .run_all()
            .await
            .assert_satisfied()
    });
}

/// Every public asynchronous contract phase remains independently pollable for
/// providers that execute without suspension.
#[test]
fn test_async_contract_entry_points_run_individually() {
    run_controlled(async {
        for phase in FileSystemContract::ALL {
            let fixture = AsyncMemoryFixture::new();
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            suite.run_contract(phase).await.assert_satisfied();
            assert!(fixture.is_empty(), "{phase:?}: phase must clean its own resources");
        }
    });
}

/// Each isolated asynchronous provider fault must fail the full suite.
#[test]
fn test_single_faults_are_rejected_by_async_suite() {
    for case in async_fault_cases() {
        let fixture = AsyncMemoryFixture::with_fault(case.fault);
        assert_panics_at(
            || {
                run_controlled(async {
                    AsyncFileSystemContractSuite::new(&fixture)
                        .run_contract(case.phase)
                        .await
                        .assert_satisfied()
                })
            },
            case.check_id,
        );
    }
}
