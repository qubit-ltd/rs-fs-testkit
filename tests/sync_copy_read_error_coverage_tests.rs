// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

pub use ::qubit_fs_testkit;
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::FileSystemContractSuite;

use self::common::MemoryFault;
use self::common::MemoryFixture;
use self::common::shared_model::FixtureHook;

/// A missing-capability basic-copy probe retains source-path preparation
/// errors.
#[test]
fn test_sync_basic_copy_attributes_source_path_preparation_error() {
    let id = ContractCheckId::CopyBasic;
    let fixture = MemoryFixture::without_core_capabilities_with_path_error(2);
    let mut suite = FileSystemContractSuite::new(&fixture);

    let run = suite.run_check(id);

    assert_eq!(run.failures().len(), 1);
    assert_eq!(run.failures()[0].check(), Some(id));
    assert!(
        run.failures()[0]
            .to_string()
            .contains("copy source path preparation failed")
    );
}

/// Basic copy reports fixture observation failures instead of accepting weak
/// evidence.
#[test]
fn test_sync_basic_copy_attributes_source_observation_error() {
    let id = ContractCheckId::CopyBasic;
    let fixture = MemoryFixture::with_fixture_hook_error(FixtureHook::ReadFile, 1);
    let mut suite = FileSystemContractSuite::new(&fixture);

    let run = suite.run_check(id);

    assert_eq!(run.failures().len(), 1);
    assert_eq!(run.failures()[0].check(), Some(id));
    assert!(run.failures()[0].to_string().contains("copy source observation failed"));
}

/// Copy conflict setup errors retain the selected check identity.
#[test]
fn test_sync_copy_conflict_attributes_source_setup_error() {
    let id = ContractCheckId::CopyFallbackOverwriteRejected;
    let fixture = MemoryFixture::with_seed_error_calls(Some(1), None);
    let mut suite = FileSystemContractSuite::new(&fixture);

    let run = suite.run_check(id);

    assert_eq!(run.failures().len(), 1);
    assert_eq!(run.failures()[0].check(), Some(id));
    assert!(
        run.failures()[0]
            .to_string()
            .contains("copy conflict preparation failed")
    );
}

/// A provider error at the accepted range boundary is reported against that
/// read check.
#[test]
fn test_sync_read_range_limit_attributes_boundary_provider_error() {
    let id = ContractCheckId::ReadRangeLimit;
    let limits = FileSystemLimits::unknown().with_max_read_range_bytes(FileSystemLimit::Maximum(4));
    let fixture = MemoryFixture::with_fault_and_limits(MemoryFault::ReadFails, limits);
    let mut suite = FileSystemContractSuite::new(&fixture);

    let run = suite.run_check(id);

    assert_eq!(run.failures().len(), 1);
    assert_eq!(run.failures()[0].check(), Some(id));
    assert!(
        run.failures()[0]
            .to_string()
            .contains("read at declared range boundary failed")
    );
    assert!(fixture.is_empty());
}
