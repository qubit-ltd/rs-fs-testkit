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

use common::{
    MemoryFault,
    MemoryFixture,
};

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

/// Verifies a conforming provider may declare a range limit below four bytes.
#[test]
fn test_range_read_contract_accepts_small_declared_limit() {
    let limits = FileSystemLimits::unknown()
        .with_max_read_range_bytes(FileSystemLimit::Maximum(3));
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

/// Verifies conditional reads reject matching `if_none_match` versions.
#[test]
#[should_panic(expected = "matching if-none-match ETags must reject reads")]
fn test_conditional_read_contract_rejects_ignored_if_none_match() {
    qubit_fs_testkit::assert_conditional_read_contract(
        &MemoryFixture::with_capabilities_and_fault(
            optional_capabilities(),
            MemoryFault::IgnoreIfNoneMatch,
        ),
    );
}

/// Verifies conditional writes reject mismatched `IfMatch` versions.
#[test]
#[should_panic(expected = "mismatched IfMatch writes must reject")]
fn test_conditional_write_contract_rejects_ignored_if_match() {
    qubit_fs_testkit::assert_conditional_write_contract(
        &MemoryFixture::with_capabilities_and_fault(
            optional_capabilities(),
            MemoryFault::IgnoreWriteIfMatch,
        ),
    );
}

/// Verifies successful conditional deletion leaves no resource behind.
#[test]
#[should_panic(expected = "matching ETags must remove the resource")]
fn test_conditional_delete_contract_rejects_successful_no_op() {
    qubit_fs_testkit::assert_conditional_delete_contract(
        &MemoryFixture::with_capabilities_and_fault(
            optional_capabilities(),
            MemoryFault::SkipConditionalDelete,
        ),
    );
}

/// Verifies a range assertion checks missing capability preflight itself.
#[test]
#[should_panic(expected = "filesystem error kind must match")]
fn test_range_read_contract_rejects_late_missing_capability_validation() {
    let capabilities = FileSystemCapabilities::default()
        .with(FileSystemCapability::Read)
        .with(FileSystemCapability::Write);
    qubit_fs_testkit::assert_range_read_contract(
        &MemoryFixture::with_capabilities_and_fault(
            capabilities,
            MemoryFault::SkipReadPreflight,
        ),
    );
}

/// Verifies conditional-read assertions check missing capability preflight.
#[test]
#[should_panic(expected = "filesystem error kind must match")]
fn test_conditional_read_contract_rejects_late_missing_capability_validation() {
    let capabilities = FileSystemCapabilities::default()
        .with(FileSystemCapability::Read)
        .with(FileSystemCapability::Write);
    qubit_fs_testkit::assert_conditional_read_contract(
        &MemoryFixture::with_capabilities_and_fault(
            capabilities,
            MemoryFault::SkipReadPreflight,
        ),
    );
}

/// Verifies checksum assertions check missing capability preflight.
#[test]
#[should_panic(expected = "filesystem error kind must match")]
fn test_checksum_validation_contract_rejects_late_missing_capability_validation()
 {
    let capabilities = FileSystemCapabilities::default()
        .with(FileSystemCapability::Read)
        .with(FileSystemCapability::Write);
    qubit_fs_testkit::assert_checksum_validation_contract(
        &MemoryFixture::with_capabilities_and_fault(
            capabilities,
            MemoryFault::SkipReadPreflight,
        ),
    );
}

/// Verifies conditional-write assertions check missing capability preflight.
#[test]
#[should_panic(
    expected = "missing write capabilities must reject before provider I/O"
)]
fn test_conditional_write_contract_rejects_late_missing_capability_validation()
{
    let capabilities = FileSystemCapabilities::default()
        .with(FileSystemCapability::Read)
        .with(FileSystemCapability::Write);
    qubit_fs_testkit::assert_conditional_write_contract(
        &MemoryFixture::with_capabilities_and_fault(
            capabilities,
            MemoryFault::SkipWritePreflight,
        ),
    );
}

/// Verifies conditional-delete assertions check missing capability preflight.
#[test]
#[should_panic(expected = "filesystem error kind must match")]
fn test_conditional_delete_contract_rejects_late_missing_capability_validation()
{
    let capabilities = FileSystemCapabilities::default()
        .with(FileSystemCapability::Read)
        .with(FileSystemCapability::Write)
        .with(FileSystemCapability::Delete);
    qubit_fs_testkit::assert_conditional_delete_contract(
        &MemoryFixture::with_capabilities_and_fault(
            capabilities,
            MemoryFault::SkipDeletePreflight,
        ),
    );
}

/// Verifies server-side-copy assertions check missing capability preflight.
#[test]
#[should_panic(expected = "filesystem error kind must match")]
fn test_server_side_copy_contract_rejects_late_missing_capability_validation() {
    let capabilities = FileSystemCapabilities::default()
        .with(FileSystemCapability::Read)
        .with(FileSystemCapability::Write)
        .with(FileSystemCapability::Copy);
    qubit_fs_testkit::assert_server_side_copy_contract(
        &MemoryFixture::with_capabilities_and_fault(
            capabilities,
            MemoryFault::SkipCopyPreflight,
        ),
    );
}
