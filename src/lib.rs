// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Reusable contract assertions for filesystem provider implementations.

#![deny(missing_docs)]

mod async_file_system_fixture;
mod async_io_contract;
mod file_system_fixture;
mod internal;
mod io_contract;
mod optional_capability_contract;
mod properties_contract;
mod representation_contract;
mod sync_file_system_contract_tests;
mod temp_contract;
mod unsupported_operations_contract;

pub use async_file_system_fixture::AsyncFileSystemFixture;
pub use async_io_contract::assert_async_write_contract;
pub use file_system_fixture::FileSystemFixture;
pub use io_contract::{
    assert_append_contract,
    assert_atomic_replace_contract,
    assert_copy_contract,
    assert_create_dir_contract,
    assert_delete_contract,
    assert_list_contract,
    assert_preflight_contract,
    assert_read_contract,
    assert_rename_contract,
    assert_stat_contract,
    assert_write_contract,
};
pub use optional_capability_contract::{
    assert_checksum_validation_contract,
    assert_conditional_delete_contract,
    assert_conditional_read_contract,
    assert_conditional_write_contract,
    assert_range_read_contract,
    assert_server_side_copy_contract,
};
pub use properties_contract::{
    assert_capabilities_contract,
    assert_properties_contract,
};
pub use representation_contract::{
    assert_empty_directory_contract,
    assert_symlink_contract,
};
pub use temp_contract::{
    assert_temp_dir_contract,
    assert_temp_file_contract,
};
pub use unsupported_operations_contract::assert_unsupported_operations_contract;
