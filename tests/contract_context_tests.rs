// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use qubit_fs_testkit::FileSystemContractSuite;

use common::MemoryFixture;

/// Repeated phases use distinct context names and cleanup all recorded paths.
#[test]
fn test_contract_context_tracks_unique_names_and_cleanup() {
    let fixture = MemoryFixture::new();
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.assert_write();
    suite.assert_write();
    assert_eq!(fixture.entry_count(), 2);

    suite.finish();
    assert!(
        fixture.is_empty(),
        "finish must clean resources created by individual phases"
    );
}
