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
//! use testkit::assert_copy_contract;
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
#[cfg(feature = "async")]
mod async_write_cancellation_stage;
#[cfg(feature = "async")]
mod async_write_fixture_case;
mod cleanup_report;
#[cfg(feature = "async")]
mod contract_async_copy_failure;
#[cfg(feature = "async")]
mod contract_async_write_failure;
mod contract_check;
mod contract_check_id;
mod contract_check_outcome;
mod contract_context;
mod contract_failure;
mod contract_registration;
mod contract_report;
mod contract_run;
mod contract_source;
mod contract_temp_failure;
mod contract_writer_failure;
#[cfg(feature = "async")]
mod copy_cancellation_probe;
mod copy_fixture_case;
mod copy_scenario;
mod delete_scenario;
mod file_system_contract;
mod file_system_contract_suite;
mod file_system_fixture;
mod fixture_error;
mod fixture_preparation;
mod fixture_support;
mod internal;
mod read_scenario;
#[cfg(feature = "async")]
mod write_cancellation_probe;
mod write_fixture_case;
mod write_scenario;

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
#[cfg(feature = "async")]
pub use async_write_cancellation_stage::AsyncWriteCancellationStage;
#[cfg(feature = "async")]
pub use async_write_fixture_case::AsyncWriteFixtureCase;
pub use cleanup_report::CleanupReport;
#[cfg(feature = "async")]
pub use contract_async_copy_failure::ContractAsyncCopyFailure;
#[cfg(feature = "async")]
pub use contract_async_write_failure::ContractAsyncWriteFailure;
pub use contract_check::ContractCheck;
pub use contract_check_id::ContractCheckId;
pub use contract_check_outcome::ContractCheckOutcome;
pub use contract_failure::ContractFailure;
pub use contract_report::ContractReport;
pub use contract_run::ContractRun;
pub use contract_source::ContractSource;
pub use contract_temp_failure::ContractTempFailure;
pub use contract_writer_failure::ContractWriterFailure;
#[cfg(feature = "async")]
pub use copy_cancellation_probe::CopyCancellationProbe;
pub use copy_fixture_case::CopyFixtureCase;
pub use copy_scenario::CopyScenario;
pub use delete_scenario::DeleteScenario;
pub use file_system_contract::FileSystemContract;
pub use file_system_contract_suite::FileSystemContractSuite;
pub use file_system_fixture::FileSystemFixture;
pub use fixture_error::FixtureError;
pub use fixture_error::FixtureResult;
pub use fixture_preparation::FixturePreparation;
pub use fixture_support::FixtureSupport;
pub use read_scenario::ReadScenario;
#[cfg(feature = "async")]
pub use write_cancellation_probe::WriteCancellationProbe;
pub use write_fixture_case::WriteFixtureCase;
pub use write_scenario::WriteScenario;
