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
    FileSystemLimit,
    FileSystemLimits,
};

use common::MemoryFixture;

/// Builds a provider capability set covering every optional contract assertion.
fn optional_capabilities() -> FileSystemCapabilities {
    FileSystemCapabilities::default()
        .with(FileSystemCapability::Read)
        .with(FileSystemCapability::RangeRead)
        .with(FileSystemCapability::ConditionalRead)
        .with(FileSystemCapability::ChecksumValidation)
        .with(FileSystemCapability::Write)
        .with(FileSystemCapability::ConditionalWrite)
        .with(FileSystemCapability::Delete)
        .with(FileSystemCapability::ConditionalDelete)
        .with(FileSystemCapability::Copy)
        .with(FileSystemCapability::ServerSideCopy)
}

/// Verifies range reads honor advertised byte-range support.
#[test]
fn test_range_read_contract_accepts_advertised_provider() {
    qubit_fs_testkit::assert_range_read_contract(
        &MemoryFixture::with_capabilities(optional_capabilities()),
    );
}

/// Verifies range reads enforce a declared finite byte limit before reading.
#[test]
fn test_range_read_contract_enforces_declared_limit() {
    let limits = FileSystemLimits::unknown()
        .with_max_read_range_bytes(FileSystemLimit::Maximum(4));
    qubit_fs_testkit::assert_range_read_contract(
        &MemoryFixture::with_capabilities_and_limits(
            optional_capabilities(),
            limits,
        ),
    );
}

/// Verifies conditional reads honor advertised version conditions.
#[test]
fn test_conditional_read_contract_accepts_advertised_provider() {
    qubit_fs_testkit::assert_conditional_read_contract(
        &MemoryFixture::with_capabilities(optional_capabilities()),
    );
}

/// Verifies checksum-required reads honor advertised validation support.
#[test]
fn test_checksum_validation_contract_accepts_advertised_provider() {
    qubit_fs_testkit::assert_checksum_validation_contract(
        &MemoryFixture::with_capabilities(optional_capabilities()),
    );
}

/// Verifies conditional writes honor advertised preconditions.
#[test]
fn test_conditional_write_contract_accepts_advertised_provider() {
    qubit_fs_testkit::assert_conditional_write_contract(
        &MemoryFixture::with_capabilities(optional_capabilities()),
    );
}

/// Verifies conditional deletes honor advertised version conditions.
#[test]
fn test_conditional_delete_contract_accepts_advertised_provider() {
    qubit_fs_testkit::assert_conditional_delete_contract(
        &MemoryFixture::with_capabilities(optional_capabilities()),
    );
}

/// Verifies required server-side copies report the requested copy method.
#[test]
fn test_server_side_copy_contract_accepts_advertised_provider() {
    qubit_fs_testkit::assert_server_side_copy_contract(
        &MemoryFixture::with_capabilities(optional_capabilities()),
    );
}
