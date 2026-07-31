// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use qubit_fs::{
    FileSystemCapability,
    FsError,
    FsErrorKind,
    FsOperation,
};
use qubit_fs_testkit::{
    FileSystemContractSuite,
    FileSystemFixture,
};

use common::MemoryFixture;

/// Both suites intentionally cover every capability in this stable order.
const COVERED_CAPABILITIES: [FileSystemCapability; 23] = [
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
    FileSystemCapability::DurableCopy,
];

/// Adding a capability to qubit-fs requires an explicit testkit coverage choice.
#[test]
fn test_contract_capability_map_is_exhaustive() {
    assert_eq!(FileSystemCapability::ALL, COVERED_CAPABILITIES);
}

/// Every advertised capability executes its positive synchronous contract.
#[test]
fn test_all_capabilities_execute_sync_contracts() {
    let fixture = MemoryFixture::with_all_capabilities();
    assert_eq!(
        fixture.file_system().properties().capabilities().iter().collect::<Vec<_>>(),
        FileSystemCapability::ALL
    );
    FileSystemContractSuite::new(&fixture).assert_all();
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
        assert!(
            fixture
                .file_system()
                .properties()
                .capabilities()
                .contains(capability)
        );
    }
    FileSystemContractSuite::new(&fixture).assert_all();
    assert!(fixture.is_empty(), "suite must clean up created resources");
}

/// A filesystem may use its own identifier as the provider identifier.
#[test]
fn test_sync_suite_allows_matching_filesystem_and_provider_ids() {
    let fixture = MemoryFixture::with_matching_ids();
    FileSystemContractSuite::new(&fixture).assert_all();
}

/// A suite must not attempt end-of-run deletion when the facade lacks it.
#[test]
fn test_sync_suite_skips_cleanup_without_delete_capability() {
    let fixture = MemoryFixture::without_delete();
    FileSystemContractSuite::new(&fixture).assert_all();
}

/// Unadvertised optional operations do not prevent core contracts from
/// exercising a conforming provider.
#[test]
fn test_sync_suite_skips_unadvertised_optional_capabilities() {
    let fixture = MemoryFixture::without_optional_capabilities();
    FileSystemContractSuite::new(&fixture).assert_all();
    assert!(
        fixture.is_empty(),
        "core contract resources must be cleaned"
    );
}

/// Each injected provider defect must be rejected by the matching suite phase.
#[test]
fn test_single_faults_are_rejected_by_sync_suite() {
    for fault in [
        common::MemoryFault::WrongStatKind,
        common::MemoryFault::KeepTempOnCleanup,
        common::MemoryFault::WrongPersistTarget,
        common::MemoryFault::EmptyList,
        common::MemoryFault::ReadWrongBytes,
        common::MemoryFault::WriteDropsBytes,
        common::MemoryFault::DeleteNoOp,
        common::MemoryFault::RenameNoOp,
    ] {
        let fixture = MemoryFixture::with_fault(fault);
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                FileSystemContractSuite::new(&fixture).assert_all();
            }));
        assert!(result.is_err(), "suite accepted injected fault: {fault:?}");
    }
}

/// A provider-native copy outcome satisfies the contract without fallback.
#[test]
fn test_sync_copy_accepts_native_outcome() {
    let fixture = MemoryFixture::with_native_copy();
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.assert_copy();
}

/// Object and prefix metadata kinds satisfy provider-neutral file and cleanup
/// contracts.
#[test]
fn test_sync_suite_accepts_object_and_prefix_kinds() {
    let fixture = MemoryFixture::with_object_kinds();
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.assert_stat();
    suite.assert_create_directory();
    suite.finish();
    assert!(fixture.is_empty(), "object resources must be cleaned");
}

/// Recursive prefix deletion does not imply directory-creation support.
#[test]
fn test_sync_recursive_delete_does_not_require_create_directory() {
    let fixture = MemoryFixture::recursive_delete_without_create_directory();
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.assert_recursive_delete();
    suite.finish();
    assert!(fixture.is_empty(), "recursive deletion must remove the prefix");
}

/// A failed assertion still cleans paths that may have been published.
#[test]
fn test_sync_suite_cleans_resources_before_resuming_panic() {
    let fixture = MemoryFixture::with_fault(common::MemoryFault::WriteDropsBytes);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        FileSystemContractSuite::new(&fixture).assert_all();
    }));
    assert!(result.is_err(), "injected write fault must fail the suite");
    assert!(fixture.is_empty(), "failed suite must clean published paths");
}

/// Unadvertised core operations still exercise the facade's structured
/// preflight.
#[test]
fn test_core_capability_negative_branches_are_exercised() {
    let fixture = MemoryFixture::without_operation_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.assert_read();
    suite.assert_write();
    suite.assert_list();
    suite.assert_create_directory();
    suite.assert_delete();
    suite.assert_copy();
    suite.assert_rename();
    assert_eq!(
        fixture.path_call_count(),
        8,
        "negative capability branches must exercise their facade paths"
    );
}

/// Unadvertised stronger operation guarantees must fail at the facade
/// preflight before a provider primitive is reached.
#[test]
fn test_stronger_capability_negative_branches_are_exercised() {
    let fixture = MemoryFixture::without_optional_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.assert_append();
    suite.assert_recursive_delete();
    suite.assert_atomic_rename();
    suite.assert_atomic_replace();
    suite.assert_durable_copy();
}

/// Each advertised option and stronger guarantee must be observed by the
/// synchronous suite rather than accepted as an unchecked provider claim.
#[test]
fn test_sync_suite_rejects_advertised_option_and_guarantee_faults() {
    for fault in [
        common::MemoryFault::ListDropsMetadata,
        common::MemoryFault::DirectoryCopyDropsChildren,
        common::MemoryFault::TempIgnoresOptions,
        common::MemoryFault::AppendOverwrites,
        common::MemoryFault::RecursiveDeleteLeavesChildren,
        common::MemoryFault::AtomicRenameNonAtomic,
        common::MemoryFault::AtomicReplaceNonAtomic,
        common::MemoryFault::DurableCopyNonDurable,
        common::MemoryFault::AtomicTempPersistNonAtomic,
        common::MemoryFault::ServerSideCopyFallsBack,
    ] {
        let fixture = MemoryFixture::with_fault(fault);
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                FileSystemContractSuite::new(&fixture).assert_all();
            }));
        assert!(
            result.is_err(),
            "suite accepted advertised option or guarantee fault: {fault:?}"
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
