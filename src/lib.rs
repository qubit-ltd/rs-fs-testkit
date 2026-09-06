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

#[cfg(feature = "async")]
mod async_copy_cancellation_stage;
#[cfg(feature = "async")]
mod async_copy_fixture_case;
#[cfg(feature = "async")]
mod async_file_system_contract_suite;
#[cfg(feature = "async")]
mod async_file_system_fixture;
mod contract_check;
mod contract_check_outcome;
mod contract_context;
mod contract_registration;
mod contract_report;
#[cfg(feature = "async")]
mod copy_cancellation_probe;
mod copy_fixture_case;
mod file_system_contract;
mod file_system_contract_suite;
mod file_system_fixture;
mod fixture_case;
mod fixture_error;
mod fixture_support;
mod internal;

#[cfg(feature = "async")]
pub use async_copy_cancellation_stage::AsyncCopyCancellationStage;
#[cfg(feature = "async")]
pub use async_copy_fixture_case::AsyncCopyFixtureCase;
#[cfg(feature = "async")]
pub use async_file_system_contract_suite::AsyncFileSystemContractSuite;
#[cfg(feature = "async")]
pub use async_file_system_fixture::AsyncFileSystemFixture;
#[cfg(feature = "async")]
pub use async_file_system_fixture::FixtureFuture;
pub use contract_check::ContractCheck;
pub use contract_check_outcome::ContractCheckOutcome;
pub use contract_report::ContractReport;
#[cfg(feature = "async")]
pub use copy_cancellation_probe::CopyCancellationProbe;
pub use copy_fixture_case::CopyFixtureCase;
pub use file_system_contract::FileSystemContract;
pub use file_system_contract_suite::FileSystemContractSuite;
pub use file_system_fixture::FileSystemFixture;
pub use fixture_case::FixtureCase;
pub use fixture_error::FixtureError;
pub use fixture_error::FixtureResult;
pub use fixture_support::FixtureSupport;
