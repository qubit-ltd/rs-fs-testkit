// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================

mod common;

use std::panic::AssertUnwindSafe;

use common::MemoryFixture;
use qubit_fs_testkit::FileSystemContractSuite;

/// An empty report has no phase evidence and therefore cannot be complete.
#[test]
fn test_empty_report_is_incomplete() {
    let fixture = MemoryFixture::new();
    let suite = FileSystemContractSuite::new(&fixture);
    let report = suite.report();
    assert!(!report.is_complete());
}

/// Strict consumers receive a useful failure for a report with no evidence.
#[test]
fn test_empty_report_strict_check_panics() {
    let fixture = MemoryFixture::new();
    let suite = FileSystemContractSuite::new(&fixture);
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| suite.report().assert_complete()));
    assert!(result.is_err());
}
