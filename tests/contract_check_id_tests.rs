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

pub use ::qubit_fs_testkit;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
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
    for id in ContractCheckId::ALL.iter().copied() {
        assert_eq!(id.to_string(), id.as_str());
        let _ = id.is_optional();
    }
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

/// A report check exposes its recorded identity, capability, and outcome.
#[test]
fn test_accessors_return_the_recorded_check_fields() {
    let fixture = MemoryFixture::new();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let check = suite
        .run_contract(FileSystemContract::List)
        .report()
        .checks()
        .iter()
        .find(|check| check.id() == ContractCheckId::ListBasic)
        .expect("list contract records its basic check");

    assert_eq!(check.id(), ContractCheckId::ListBasic);
    assert_eq!(check.capability(), Some(FileSystemCapability::List));
    assert_eq!(check.outcome(), &ContractCheckOutcome::Passed);
}
