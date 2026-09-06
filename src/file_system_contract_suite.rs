// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stateful synchronous filesystem provider contract suite.

use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::panic::resume_unwind;

use qubit_fs::copy::CopyConflictPolicy;
use qubit_fs::copy::CopyMethod;
use qubit_fs::copy::CopyMode;
use qubit_fs::copy::CopyOptions;
use qubit_fs::copy::ServerSidePreference;
use qubit_fs::directory::CreateDirectoryOptions;
use qubit_fs::directory::DeleteOptions;
use qubit_fs::directory::ListOptions;
use qubit_fs::error::FsError;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::AchievedAtomicity;
use qubit_fs::metadata::AtomicityRequirement;
use qubit_fs::metadata::DurabilityRequirement;
use qubit_fs::metadata::FileKind;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::ResourceVersion;
use qubit_fs::path::Path;
use qubit_fs::path::PathSemantics;
use qubit_fs::read::ChecksumPolicy;
use qubit_fs::read::ReadOptions;
use qubit_fs::rename::RenameFailureState;
use qubit_fs::rename::RenameOptions;
use qubit_fs::temp::PersistFailureState;
use qubit_fs::temp::PersistOptions;
use qubit_fs::temp::TempDirectory;
use qubit_fs::temp::TempFile;
use qubit_fs::temp::TempOptions as TempDirectoryOptions;
use qubit_fs::temp::TempOptions as TempFileOptions;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteOptions;
use qubit_fs::write::WritePrecondition;
use qubit_io::Output;

use crate::ContractReport;
use crate::FileSystemContract;
use crate::FileSystemFixture;
use crate::FixtureSupport;
use crate::contract_context::ContractContext;
use crate::internal::assert_error_with_source_or_target;
use crate::internal::assert_error_with_target;
use crate::internal::assert_unsupported_error;
// Implements property snapshots and bounded limit checks.
mod properties;
// Implements reader contracts.
mod read;
// Implements writer and publication contracts.
mod write;
// Implements namespace and metadata contracts.
mod namespace;
// Implements copy contracts.
mod copy;
// Implements temporary resource contracts.
mod temp;
// Implements fixture adaptation and suite lifecycle support.
mod support;

/// Runs synchronous provider contracts against one isolated fixture.
///
/// # Type Parameters
///
/// * `'a` - Lifetime of the borrowed provider fixture.
#[must_use = "the suite must run at least one contract assertion"]
pub struct FileSystemContractSuite<'a> {
    /// Provider-owned fixture supplying the facade and observation hooks.
    fixture: &'a dyn FileSystemFixture,
    /// Property snapshot and cleanup state for the current suite run.
    context: ContractContext,
    teardown_completed: bool,
}

impl<'a> FileSystemContractSuite<'a> {
    /// Creates a stateful suite borrowing one isolated provider fixture.
    ///
    /// # Parameters
    ///
    /// * `fixture` - Isolated provider fixture exercised by the suite.
    ///
    /// # Returns
    ///
    /// A suite with a fresh context and captured property snapshot.
    #[inline]
    pub fn new(fixture: &'a dyn FileSystemFixture) -> Self {
        Self {
            fixture,
            context: ContractContext::new(fixture.file_system().properties()),
            teardown_completed: false,
        }
    }

    /// Runs all synchronous contracts in their dependency-safe fixed order.
    ///
    /// # Panics
    ///
    /// Panics when the provider violates any contract or fixture setup and
    /// observation fails. Cleanup still runs before the panic is resumed.
    pub fn assert_all(self) {
        let _ = self.assert_all_with_report();
    }

    /// Runs one named synchronous contract and always performs cleanup.
    ///
    /// # Panics
    ///
    /// Panics when the provider violates the selected contract or cleanup
    /// fails. Cleanup runs before an assertion panic is resumed.
    pub fn assert_contract(self, contract: FileSystemContract) {
        let _ = self.assert_contract_with_report(contract);
    }

    /// Runs all phases, performs cleanup, and returns the execution report.
    pub fn assert_all_with_report(mut self) -> ContractReport {
        let result = catch_unwind(AssertUnwindSafe(|| {
            for contract in FileSystemContract::ALL {
                self.assert_contract_inner(contract);
            }
        }));
        let cleanup = catch_unwind(AssertUnwindSafe(|| self.finish()));
        if let Err(payload) = result {
            resume_unwind(payload);
        }
        if let Err(payload) = cleanup {
            resume_unwind(payload);
        }
        self.context.report().clone()
    }

    /// Runs one phase, performs cleanup, and returns the execution report.
    pub fn assert_contract_with_report(mut self, contract: FileSystemContract) -> ContractReport {
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.assert_contract_inner(contract);
        }));
        let cleanup = catch_unwind(AssertUnwindSafe(|| self.finish()));
        if let Err(payload) = result {
            resume_unwind(payload);
        }
        if let Err(payload) = cleanup {
            resume_unwind(payload);
        }
        self.context.report().clone()
    }

    /// Returns the report currently accumulated by this suite.
    #[inline]
    pub fn report(&self) -> &ContractReport {
        self.context.report()
    }

    /// Dispatches one named phase without changing cleanup ownership.
    fn assert_contract_inner(&mut self, contract: FileSystemContract) {
        self.context.prepare_phase(contract);
        match contract {
            FileSystemContract::Properties => self.assert_properties(),
            FileSystemContract::Stat => self.assert_stat(),
            FileSystemContract::Read => self.assert_read(),
            FileSystemContract::Write => self.assert_write(),
            FileSystemContract::List => self.assert_list(),
            FileSystemContract::CreateDirectory => self.assert_create_directory(),
            FileSystemContract::Representations => self.assert_representations(),
            FileSystemContract::Delete => self.assert_delete(),
            FileSystemContract::Copy => self.assert_copy(),
            FileSystemContract::Rename => self.assert_rename(),
            FileSystemContract::Append => self.assert_append(),
            FileSystemContract::RecursiveDelete => self.assert_recursive_delete(),
            FileSystemContract::AtomicRename => self.assert_atomic_rename(),
            FileSystemContract::DurableRename => self.assert_durable_rename(),
            FileSystemContract::AtomicReplace => self.assert_atomic_replace(),
            FileSystemContract::DurableFileCopy => self.assert_durable_copy(),
            FileSystemContract::TempResources => self.assert_temp_resources(),
            FileSystemContract::ErrorContext => self.assert_error_context(),
        }
        let capabilities = self.context.properties().capabilities();
        self.context.report_mut().complete_phase(contract, &capabilities);
    }
}
