// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use qubit_fs::{FileSystemCapabilities, FileSystemCapability};

use common::MemoryFixture;

/// Verifies advertised empty-directory representation is observable through stat.
#[test]
fn test_empty_directory_contract_accepts_conforming_provider() {
    let capabilities = FileSystemCapabilities::default().with(FileSystemCapability::EmptyDirectory);

    qubit_fs_testkit::assert_empty_directory_contract(&MemoryFixture::with_capabilities(
        capabilities,
    ));
}

/// Verifies advertised symbolic links remain visible to stat without dereferencing.
#[test]
fn test_symlink_contract_accepts_conforming_provider() {
    let capabilities = FileSystemCapabilities::default().with(FileSystemCapability::Symlink);

    qubit_fs_testkit::assert_symlink_contract(&MemoryFixture::with_capabilities(capabilities));
}
