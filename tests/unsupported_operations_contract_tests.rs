// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use qubit_fs::{
    FileSystemCapabilities,
    FileSystemCapability,
};

use common::MemoryFixture;

/// Verifies unsupported operations expose structured capability errors.
#[test]
fn test_unsupported_operations_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_unsupported_operations_contract(
        &MemoryFixture::new(),
    );
}

/// Verifies the unsupported-operation contract also checks reader support.
#[test]
#[should_panic(expected = "filesystem error kind must match")]
fn test_unsupported_operations_contract_rejects_incorrect_read_error() {
    let fixture =
        MemoryFixture::with_capabilities(FileSystemCapabilities::default());

    qubit_fs_testkit::assert_unsupported_operations_contract(&fixture);
}

/// Verifies the unsupported-operation contract also checks writer support.
#[test]
#[should_panic(expected = "unadvertised write must fail")]
fn test_unsupported_operations_contract_rejects_incorrect_write_error() {
    let capabilities =
        FileSystemCapabilities::default().with(FileSystemCapability::Read);
    let fixture = MemoryFixture::with_capabilities(capabilities);

    qubit_fs_testkit::assert_unsupported_operations_contract(&fixture);
}
