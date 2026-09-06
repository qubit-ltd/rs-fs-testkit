// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================

mod common;

#[cfg(feature = "async")]
use common::AsyncMemoryFixture;
use common::MemoryFixture;
#[cfg(feature = "async")]
use common::async_memory_file_system::run_controlled;
#[cfg(feature = "async")]
use qubit_fs_testkit::register_async_file_system_contract_tests;
use qubit_fs_testkit::register_file_system_contract_tests;

register_file_system_contract_tests! {
    module: registered_sync_contracts,
    fixture: super::MemoryFixture::new,
}

#[cfg(feature = "async")]
register_async_file_system_contract_tests! {
    module: registered_async_contracts,
    fixture: super::AsyncMemoryFixture::new,
    runner: super::run_controlled,
}
