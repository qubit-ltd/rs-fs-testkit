// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================

mod common;
use std::panic::AssertUnwindSafe;

use qubit_fs_testkit::FileSystemContractSuite;

use self::common::MemoryFixture;
/// An empty report has no phase evidence and therefore cannot be complete.
#[test]
fn test_empty_report_is_incomplete() {
    let fixture = MemoryFixture::new();
    let suite = FileSystemContractSuite::new(&fixture);
    let report = suite.report();
    assert!(!report.requirements_satisfied());
}

/// Strict consumers receive a useful failure for a report with no evidence.
#[test]
fn test_empty_report_strict_check_panics() {
    let fixture = MemoryFixture::new();
    let suite = FileSystemContractSuite::new(&fixture);
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| suite.report().assert_satisfied()));
    assert!(result.is_err());
}

/// A synchronous provider never inherits asynchronous cancellation obligations.
#[test]
fn test_sync_write_catalog_excludes_async_checks() {
    let fixture = MemoryFixture::new();
    let report = FileSystemContractSuite::new(&fixture).assert_contract_with_report(FileSystemContract::Write);
    report.assert_complete();
    for id in [
        "write/owning-operation",
        "write/repeated-execute",
        "write/cancel-open",
        "write/cancel-write",
        "write/cancel-flush",
        "write/cancel-commit",
    ] {
        assert!(
            !report.checks().iter().any(|check| check.id() == id),
            "sync catalog included {id}"
        );
    }
}
