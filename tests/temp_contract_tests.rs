// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_fs_testkit::{FileSystemFixture, assert_temp_dir_contract, assert_temp_file_contract};

#[test]
fn test_temp_contract_assertions_are_exported() {
    let _: fn(&dyn FileSystemFixture) = assert_temp_file_contract;
    let _: fn(&dyn FileSystemFixture) = assert_temp_dir_contract;
}
