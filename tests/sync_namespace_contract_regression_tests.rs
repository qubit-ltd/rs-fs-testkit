// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
pub use ::qubit_fs_testkit;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;

use self::common::MemoryFixture;
mod common;

/// A provider exposing only the stream fallback still passes the applicable
/// copy contract and records the native overwrite boundary precisely.
#[test]
fn test_sync_accepts_fallback_only_copy() {
    let fixture = MemoryFixture::fallback_only();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Copy);
    run.assert_satisfied();
    let report = run.report();
    assert!(report.checks().iter().any(|check| {
        check.id().as_str() == "copy/fallback-overwrite-rejected"
            && matches!(check.outcome(), ContractCheckOutcome::RejectedAsExpected)
    }));
    assert!(fixture.is_empty(), "fallback copy contract leaked resources");
}

/// A read-only provider can satisfy Copy through its native operation.
#[test]
fn test_sync_read_only_supports_native_copy_without_write() {
    let fixture = MemoryFixture::read_only();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Copy);
    run.assert_satisfied();
    assert!(run.report().checks().iter().any(|check| {
        check.id().as_str() == "copy/basic" && matches!(check.outcome(), ContractCheckOutcome::Passed)
    }));
    assert!(fixture.is_empty(), "read-only native copy contract leaked resources");
}
