// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public report identities must come from the exhaustive typed check set.

mod common;
use std::collections::HashSet;

use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;

use self::common::MemoryFixture;
/// Stable text must identify exactly one typed check.
#[test]
fn test_check_names_are_unique() {
    let names = ContractCheckId::ALL
        .iter()
        .map(|id| id.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(
        names.len(),
        ContractCheckId::ALL.len(),
        "check identities must not alias"
    );
    assert!(names.contains("write/cancel-open"));
    assert!(names.contains("write/repeated-execute"));
}

/// Callers receive a typed identity, not an arbitrary runtime string.
#[test]
fn test_report_returns_registered_check_identity() {
    let fixture = MemoryFixture::new();
    let report = FileSystemContractSuite::new(&fixture)
        .run_contract(FileSystemContract::ErrorContext)
        .report()
        .clone();
    let id: ContractCheckId = report.checks()[0].id();
    assert_eq!(id, ContractCheckId::ErrorContext);
    assert_eq!(id.as_str(), "error/context");
}
