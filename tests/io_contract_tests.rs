// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use qubit_fs::{
    AtomicityRequirement, FileSystemCapabilities, FileSystemCapability, WriteDisposition,
    WriteOptions,
};
use qubit_fs_testkit::FileSystemFixture;

use common::MemoryFixture;

/// Verifies the stat contract accepts a conforming provider.
#[test]
fn test_stat_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_stat_contract(&MemoryFixture::new());
}

/// Verifies the read contract accepts a conforming provider.
#[test]
fn test_read_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_read_contract(&MemoryFixture::new());
}

/// Verifies the write contract accepts a conforming provider.
#[test]
fn test_write_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_write_contract(&MemoryFixture::new());
}

/// Verifies the append contract accepts a conforming provider.
#[test]
fn test_append_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_append_contract(&MemoryFixture::new());
}

/// Verifies append rejection preserves the required capability context.
#[test]
fn test_append_contract_accepts_conforming_rejection() {
    let capabilities = FileSystemCapabilities::default()
        .with(FileSystemCapability::Read)
        .with(FileSystemCapability::Write);
    let fixture = MemoryFixture::with_capabilities(capabilities);
    let file_system = fixture.file_system();
    assert!(
        !file_system
            .capabilities()
            .contains(FileSystemCapability::Append)
    );
    let options = WriteOptions {
        disposition: WriteDisposition::Append,
        atomicity: AtomicityRequirement::NotRequired,
        ..WriteOptions::default()
    };
    assert!(
        options
            .validate_against(file_system.capabilities())
            .is_err()
    );

    qubit_fs_testkit::assert_append_contract(&fixture);
}

/// Verifies the atomic-replace contract accepts a conforming provider.
#[test]
fn test_atomic_replace_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_atomic_replace_contract(&MemoryFixture::new());
}

/// Verifies atomic-replace rejection preserves required capability context.
#[test]
fn test_atomic_replace_contract_accepts_conforming_rejection() {
    let capabilities = FileSystemCapabilities::default()
        .with(FileSystemCapability::Read)
        .with(FileSystemCapability::Write);
    let fixture = MemoryFixture::with_capabilities(capabilities);
    let file_system = fixture.file_system();
    assert!(
        !file_system
            .capabilities()
            .contains(FileSystemCapability::AtomicReplace)
    );
    let options = WriteOptions {
        disposition: WriteDisposition::CreateOrReplace,
        atomicity: AtomicityRequirement::Required,
        ..WriteOptions::default()
    };
    assert!(
        options
            .validate_against(file_system.capabilities())
            .is_err()
    );

    qubit_fs_testkit::assert_atomic_replace_contract(&fixture);
}

/// Verifies preflight checks run before provider I/O.
#[test]
fn test_preflight_contract_accepts_conforming_provider() {
    let capabilities = FileSystemCapabilities::default()
        .with(FileSystemCapability::Read)
        .with(FileSystemCapability::Write);
    qubit_fs_testkit::assert_preflight_contract(&MemoryFixture::with_capabilities(capabilities));
}
