// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================

mod common;

use qubit_fs::{FileSystemCapability, FsError, FsErrorKind, FsOperation};
use qubit_fs_testkit::{FileSystemContractSuite, FileSystemFixture};

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
    ] {
        let fixture = MemoryFixture::with_fault(fault);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            FileSystemContractSuite::new(&fixture).assert_all();
        }));
        assert!(result.is_err(), "suite accepted injected fault: {fault:?}");
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
