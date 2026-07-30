// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stateful contract suites for `qubit-fs` provider implementations.
//!
//! ```compile_fail
//! use qubit_fs_testkit::assert_copy_contract;
//! ```

#![deny(missing_docs)]

mod async_copy_cancellation_stage;
mod async_copy_fixture_case;
mod async_file_system_contract_suite;
mod async_file_system_fixture;
mod contract_context;
mod copy_fixture_case;
mod file_system_contract_suite;
mod file_system_fixture;
mod fixture_error;
mod fixture_support;

pub use async_copy_cancellation_stage::AsyncCopyCancellationStage;
pub use async_copy_fixture_case::AsyncCopyFixtureCase;
pub use async_file_system_contract_suite::AsyncFileSystemContractSuite;
pub use async_file_system_fixture::{
    AsyncFileSystemFixture,
    FixtureFuture,
};
pub use copy_fixture_case::CopyFixtureCase;
pub use file_system_contract_suite::FileSystemContractSuite;
pub use file_system_fixture::FileSystemFixture;
pub use fixture_error::{
    FixtureError,
    FixtureResult,
};
pub use fixture_support::FixtureSupport;
