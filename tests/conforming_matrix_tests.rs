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

/// A conforming provider must satisfy every synchronous suite phase.
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

/// Each injected provider defect must be rejected by the matching suite phase.
#[test]
fn test_single_faults_are_rejected_by_sync_suite() {
    for fault in [
        common::MemoryFault::WrongStatKind,
        common::MemoryFault::KeepTempOnCleanup,
        common::MemoryFault::WrongPersistTarget,
        common::MemoryFault::EmptyList,
    ] {
        let fixture = MemoryFixture::with_fault(fault);
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                FileSystemContractSuite::new(&fixture).assert_all();
            }));
        assert!(result.is_err(), "suite accepted injected fault: {fault:?}");
    }
}

/// Repeated assertion phases must use distinct resource names.
#[test]
fn test_sync_suite_uses_unique_names_for_repeated_phases() {
    let fixture = MemoryFixture::new();
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.assert_write();
    suite.assert_write();
    assert_eq!(fixture.entry_count(), 2);
}

/// Unadvertised core operations still exercise the facade's structured
/// preflight.
#[test]
fn test_core_capability_negative_branches_are_exercised() {
    let fixture = MemoryFixture::without_core_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.assert_read();
    suite.assert_write();
    suite.assert_list();
    assert_eq!(
        fixture.path_call_count(),
        3,
        "each negative capability branch must exercise one facade path"
    );
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
