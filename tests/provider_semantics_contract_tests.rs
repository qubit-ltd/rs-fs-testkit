// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use qubit_fs::{FileSystemCapabilities, FileSystemCapability, PathSemantics};

use common::MemoryFixture;

/// Verifies object-key providers satisfy provider-neutral I/O contracts.
#[test]
fn test_object_key_provider_satisfies_stat_list_and_create_dir_contracts() {
    let capabilities = FileSystemCapabilities::default()
        .with(FileSystemCapability::Write)
        .with(FileSystemCapability::List)
        .with(FileSystemCapability::CreateDirectory);
    let fixture =
        MemoryFixture::with_capabilities_and_path_semantics(capabilities, PathSemantics::ObjectKey);

    qubit_fs_testkit::assert_stat_contract(&fixture);
    qubit_fs_testkit::assert_list_contract(&fixture);
    qubit_fs_testkit::assert_create_dir_contract(&fixture);
}
