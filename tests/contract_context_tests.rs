// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;

use self::common::MemoryFixture;
/// Repeated phases use distinct context names and cleanup all recorded paths.
#[test]
fn test_contract_context_tracks_unique_names_and_cleanup() {
    let fixture = MemoryFixture::new();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let first = suite.run_contract(FileSystemContract::Write);
    assert!(
        first.requirements_satisfied(),
        "write run should clean its resources automatically"
    );
    assert!(fixture.is_empty(), "completed runs must drain tracked resources");
    let run = suite.run_contract(FileSystemContract::Write);
    assert!(!run.requirements_satisfied(), "a second run must be rejected");
    assert!(fixture.is_empty(), "a rejected second run must not execute writes");

    suite.finish();
    assert!(
        fixture.is_empty(),
        "finish must clean resources created by individual phases"
    );
}
