// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;
use qubit_fs::error::FsError;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs_testkit as testkit;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;
use qubit_fs_testkit::FileSystemFixture;

use self::common::MemoryFault;
use self::common::MemoryFixture;
use self::common::check_matrix::assert_panics_at;
use self::common::check_matrix::sync_fault_cases;
use crate::common::UnavailableScenario;
/// Both suites intentionally cover every capability in this stable order.
const COVERED_CAPABILITIES: [FileSystemCapability; 28] = [
    FileSystemCapability::List,
    FileSystemCapability::Read,
    FileSystemCapability::RangeRead,
    FileSystemCapability::ConditionalRead,
    FileSystemCapability::ChecksumValidation,
    FileSystemCapability::Write,
    FileSystemCapability::Append,
    FileSystemCapability::ConditionalWrite,
    FileSystemCapability::CreateDirectory,
    FileSystemCapability::EmptyDirectory,
    FileSystemCapability::Delete,
    FileSystemCapability::RecursiveDelete,
    FileSystemCapability::ConditionalDelete,
    FileSystemCapability::Rename,
    FileSystemCapability::AtomicRename,
    FileSystemCapability::AtomicReplace,
    FileSystemCapability::Copy,
    FileSystemCapability::ServerSideCopy,
    FileSystemCapability::Symlink,
    FileSystemCapability::TempFile,
    FileSystemCapability::TempDirectory,
    FileSystemCapability::AtomicTempPersist,
    FileSystemCapability::AtomicFileCopy,
    FileSystemCapability::AtomicTreeCopy,
    FileSystemCapability::DurableFileCopy,
    FileSystemCapability::DurableTreeCopy,
    FileSystemCapability::DurableRename,
    FileSystemCapability::DurableWrite,
];

/// Adding a capability to qubit-fs requires an explicit testkit coverage
/// choice.
#[test]
fn test_contract_capability_map_is_exhaustive() {
    assert_eq!(FileSystemCapability::ALL, COVERED_CAPABILITIES);
}

/// Every advertised capability executes its positive synchronous contract.
#[test]
fn test_all_capabilities_execute_sync_contracts() {
    let fixture = MemoryFixture::with_all_capabilities();
    assert_eq!(
        fixture
            .file_system()
            .properties()
            .capabilities()
            .iter()
            .collect::<Vec<_>>(),
        FileSystemCapability::ALL.to_vec()
    );
    FileSystemContractSuite::new(&fixture).run_all().assert_satisfied();
    assert!(fixture.is_empty(), "all-capability suite must clean up");
}

/// A conforming provider satisfies every synchronous suite phase.
#[test]
fn test_conforming_memory_provider_satisfies_sync_suite() {
    let fixture = MemoryFixture::new();
    for capability in [
        FileSystemCapability::Read,
        FileSystemCapability::Write,
        FileSystemCapability::List,
        FileSystemCapability::CreateDirectory,
        FileSystemCapability::Delete,
        FileSystemCapability::Copy,
        FileSystemCapability::Rename,
        FileSystemCapability::TempFile,
        FileSystemCapability::TempDirectory,
    ] {
        assert!(fixture.file_system().properties().capabilities().supports(capability));
    }
    FileSystemContractSuite::new(&fixture).run_all().assert_satisfied();
    assert!(fixture.is_empty(), "suite must clean up created resources");
}

#[test]
fn test_sync_phase_matrix_exercises_declared_profiles() {
    for contract in FileSystemContract::ALL {
        for profile in [0_u8, 1, 2, 3, 4] {
            let fixture = match profile {
                0 => MemoryFixture::with_all_capabilities(),
                1 => MemoryFixture::without_operation_capabilities(),
                2 => MemoryFixture::without_optional_capabilities(),
                3 => MemoryFixture::fallback_only(),
                _ => MemoryFixture::read_only(),
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                FileSystemContractSuite::new(&fixture)
                    .run_contract(contract)
                    .assert_satisfied();
            }));
            assert!(result.is_ok(), "sync profile {profile} panicked in {contract:?}");
        }
    }
}

#[test]
fn test_sync_faults_exercise_full_suite_paths() {
    for case in sync_fault_cases() {
        for contract in FileSystemContract::ALL {
            let fixture = MemoryFixture::with_fault(case.fault);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                FileSystemContractSuite::new(&fixture)
                    .run_contract(contract)
                    .assert_satisfied();
            }));
        }
    }
}

#[test]
fn test_sync_property_profiles_cover_all_limit_outcomes() {
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
            let fixture = MemoryFixture::with_limits(snapshot);
            FileSystemContractSuite::new(&fixture)
                .run_contract(FileSystemContract::Properties)
                .assert_satisfied();
        }));
    }
}

#[test]
fn test_sync_unavailable_fixture_cases_are_exercised() {
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
        let fixture = MemoryFixture::with_conditional_case_unavailable(case);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            FileSystemContractSuite::new(&fixture).run_all().assert_satisfied();
        }));
    }
}

/// A filesystem may use its own identifier as the provider identifier.
#[test]
fn test_sync_suite_allows_matching_filesystem_and_provider_ids() {
    let fixture = MemoryFixture::with_matching_ids();
    FileSystemContractSuite::new(&fixture).run_all().assert_satisfied();
}

/// A suite must not attempt end-of-run deletion when the facade lacks it.
#[test]
fn test_sync_suite_skips_cleanup_without_delete_capability() {
    let fixture = MemoryFixture::without_delete();
    FileSystemContractSuite::new(&fixture).run_all().assert_satisfied();
}

/// Unadvertised optional operations do not prevent core contracts from
/// exercising a conforming provider.
#[test]
fn test_sync_suite_skips_unadvertised_optional_capabilities() {
    let fixture = MemoryFixture::without_optional_capabilities();
    FileSystemContractSuite::new(&fixture).run_all().assert_satisfied();
    assert!(fixture.is_empty(), "core contract resources must be cleaned");
}

/// Each injected provider defect must be rejected by the matching suite phase.
#[test]
fn test_single_faults_are_rejected_by_sync_suite() {
    for case in sync_fault_cases() {
        let fixture = MemoryFixture::with_fault(case.fault);
        assert_panics_at(
            || {
                FileSystemContractSuite::new(&fixture)
                    .run_contract(case.phase)
                    .assert_satisfied()
            },
            case.check_id,
        );
    }
}

/// A provider-native copy outcome satisfies the contract without fallback.
#[test]
fn test_sync_copy_accepts_native_outcome() {
    let fixture = MemoryFixture::with_native_copy();
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.run_contract(FileSystemContract::Copy).assert_satisfied();
}

/// Object and prefix metadata kinds satisfy provider-neutral file and cleanup
/// contracts.
#[test]
fn test_sync_suite_accepts_object_and_prefix_kinds() {
    for phase in [FileSystemContract::Stat, FileSystemContract::CreateDirectory] {
        let fixture = MemoryFixture::with_object_kinds();
        let mut suite = FileSystemContractSuite::new(&fixture);
        suite.run_contract(phase).assert_satisfied();
        assert!(fixture.is_empty(), "object resources must be cleaned");
    }
}

/// Recursive prefix deletion does not imply directory-creation support.
#[test]
fn test_sync_recursive_delete_does_not_require_create_directory() {
    let fixture = MemoryFixture::recursive_delete_without_create_directory();
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.run_check(testkit::ContractCheckId::DeleteTree).assert_satisfied();
    assert!(fixture.is_empty(), "recursive deletion must remove the prefix");
}

/// A failed assertion still cleans paths that may have been published.
#[test]
fn test_sync_suite_cleans_resources_before_resuming_panic() {
    let fixture = MemoryFixture::with_fault(MemoryFault::WriteDropsBytes);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        FileSystemContractSuite::new(&fixture).run_all().assert_satisfied();
    }));
    assert!(result.is_err(), "injected write fault must fail the suite");
    assert!(fixture.is_empty(), "failed suite must clean published paths");
}

/// Unadvertised core operations still exercise the facade's structured
/// preflight.
#[test]
fn test_core_capability_negative_branches_are_exercised() {
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
        let fixture = MemoryFixture::without_operation_capabilities();
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(id.contract());
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
}

/// Unadvertised stronger operation guarantees must fail at the facade
/// preflight before a provider primitive is reached.
#[test]
fn test_stronger_capability_negative_branches_are_exercised() {
    use qubit_fs_testkit::ContractCheckId;
    for id in [
        ContractCheckId::AppendBasic,
        ContractCheckId::DeleteTree,
        ContractCheckId::RenameAtomic,
        ContractCheckId::RenameDurable,
        ContractCheckId::WriteAtomicReplaceExisting,
        ContractCheckId::CopyDurableFile,
        ContractCheckId::CopyDurableTree,
    ] {
        let fixture = MemoryFixture::without_optional_capabilities();
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        run.assert_satisfied();
        assert!(
            matches!(
                run.report().checks()[0].outcome(),
                testkit::ContractCheckOutcome::RejectedAsExpected
            ),
            "{id}"
        );
        assert!(fixture.is_empty());
    }
}

/// Each advertised option and stronger guarantee must be observed by the
/// synchronous suite rather than accepted as an unchecked provider claim.
#[test]
fn test_sync_suite_rejects_advertised_option_and_guarantee_faults() {
    for case in sync_fault_cases().iter().skip(8) {
        let fixture = MemoryFixture::with_fault(case.fault);
        assert_panics_at(
            || {
                FileSystemContractSuite::new(&fixture)
                    .run_contract(case.phase)
                    .assert_satisfied()
            },
            case.check_id,
        );
    }
}

/// Structured errors must not format an untrusted source diagnostic.
#[test]
fn test_error_formatting_redacts_nested_secret() {
    let error = FsError::with_source(
        FsErrorKind::Io,
        FsOperation::Stat,
        "safe provider message",
        std::io::Error::other("token=contract-secret"),
    );
    assert!(!format!("{error}").contains("contract-secret"));
    assert!(!format!("{error:?}").contains("contract-secret"));
}
