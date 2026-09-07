// qubit-style: allow explicit-imports
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

pub(crate) use qubit_fs::error::FsError;
pub(crate) use qubit_fs::error::FsErrorKind;
pub(crate) use qubit_fs::error::FsOperation;
pub(crate) use qubit_fs::metadata::FileSystemCapability;
pub(crate) use qubit_fs::path::Path;
pub(crate) use qubit_fs::write::WriteDisposition;

use crate::ContractCheckOutcome;
use crate::ContractReport;
use crate::FileSystemContract;
use crate::FileSystemFixture;
use crate::FixtureSupport;
use crate::contract_context::ContractContext;
pub(crate) use crate::internal::assert_error_with_source_or_target;
pub(crate) use crate::internal::assert_error_with_target;
pub(crate) use crate::internal::assert_unsupported_error;
// Implements property snapshots and bounded limit checks.
mod properties;
// Implements reader contracts.
mod read;
// Implements writer and publication contracts.
mod write;
// Implements independently prepared bounded write evidence.
mod write_limit;
// Implements explicit conditional creation evidence.
mod write_if_absent;
// Implements explicit version-conditional replacement evidence.
mod write_if_match;
// Implements strong write guarantee evidence.
mod write_guarantee;
// Implements independent append evidence.
mod append;
// Implements independently seeded creation conflicts.
mod create_conflict;
// Implements independently prepared replacement and truncation.
mod replace;
// Implements independently prepared explicit abort.
mod abort;
// Implements namespace and metadata contracts.
// Implements independently prepared metadata checks.
mod stat;
// Implements independently observed directory creation.
mod create_directory;
// Implements independent representation checks.
mod representations;
// Implements independent deletion checks.
mod delete;
// Implements independently prepared recursive deletion.
mod delete_tree;
// Implements independent rename requirements.
mod rename;
// Implements independent hierarchy and object listing.
mod list;
// Implements copy contracts.
mod basic_copy;
mod copy;
mod copy_conflict;
mod server_side_copy;
mod strong_file_copy;
mod strong_tree_copy;
// Implements temporary resource contracts.
mod temp;
mod temp_directory_check;
mod temp_file_check;
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

    /// Returns the result retained by this suite, including a failed session.
    pub const fn run(&self) -> &crate::ContractRun {
        &self.context.run
    }

    /// Executes all phases once and retains both execution and cleanup
    /// failures.
    pub fn run_all(&mut self) -> &crate::ContractRun {
        self.run_selected(&FileSystemContract::ALL, None)
    }

    /// Executes one phase once and retains its result in the suite.
    pub fn run_contract(&mut self, contract: FileSystemContract) -> &crate::ContractRun {
        self.run_selected(&[contract], None)
    }

    /// Executes one selected check in a fresh session and retains its cleanup
    /// result.
    pub fn run_check(&mut self, id: crate::ContractCheckId) -> &crate::ContractRun {
        self.run_selected(&[id.contract()], Some(id))
    }

    fn run_selected(
        &mut self,
        contracts: &[FileSystemContract],
        selected: Option<crate::ContractCheckId>,
    ) -> &crate::ContractRun {
        if self.context.run.started {
            self.context.run.failures.push(crate::ContractFailure::message_only(
                "session already started; create a fresh fixture for another run",
            ));
            return &self.context.run;
        }
        self.context.run.started = true;
        // Register the entire selection before entering provider code.
        if let Some(id) = selected {
            let spec = crate::internal::check_catalog::specification(id);
            self.context.run.report.register(id, spec.capability);
            self.context.begin(id.as_str());
        } else {
            for contract in contracts {
                self.context.prepare_phase(*contract, false);
            }
        }
        for contract in contracts {
            let result = catch_unwind(AssertUnwindSafe(|| match selected {
                Some(id) => self.dispatch_check(id),
                None => self.dispatch_contract(*contract),
            }));
            match result {
                Ok(Ok(())) => {}
                Ok(Err(failure)) => {
                    self.context.fail(failure);
                    break;
                }
                Err(payload) => {
                    let mut failure = crate::ContractFailure::panicked("contract execution panicked", payload);
                    if let Some(id) = selected {
                        failure = failure.at(id);
                    }
                    self.context.fail(failure);
                    break;
                }
            }
        }
        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| self.finish())) {
            self.context
                .run
                .failures
                .push(crate::ContractFailure::panicked("cleanup failed", payload));
        }
        self.context.run.completed = true;
        &self.context.run
    }

    /// Returns the report currently accumulated by this suite.
    #[inline]
    pub fn report(&self) -> &ContractReport {
        self.context.report()
    }

    /// Resolves focused execution without running sibling checks.
    fn dispatch_check(&mut self, id: crate::ContractCheckId) -> Result<(), crate::ContractFailure> {
        if !id.supports_synchronous() {
            return Err(crate::ContractFailure::message_only("selected check requires asynchronous execution").at(id));
        }
        match id.contract() {
            FileSystemContract::Copy => self.check_copy_item(id),
            FileSystemContract::TempResources => self.check_temp_item(id),
            FileSystemContract::List => self.check_list_item(id),
            FileSystemContract::Rename => self.check_rename_item(id),
            FileSystemContract::Delete => self.check_delete_item(id),
            FileSystemContract::Representations => self.check_representation_item(id),
            FileSystemContract::CreateDirectory => self.check_create_directory_item(id),
            FileSystemContract::Properties => self.check_properties_item(id),
            FileSystemContract::Stat => self.check_stat_item(id),
            FileSystemContract::Read => self.check_read_item(id),
            FileSystemContract::Write => self.check_write_item(id),
            FileSystemContract::ErrorContext => self.check_error_context(),
        }
    }

    fn dispatch_contract(&mut self, contract: FileSystemContract) -> Result<(), crate::ContractFailure> {
        match contract {
            FileSystemContract::Properties => self.check_properties()?,
            FileSystemContract::Stat => self.check_stat()?,
            FileSystemContract::Read => self.check_read()?,
            FileSystemContract::Write => {
                self.check_write()?;
            }
            FileSystemContract::List => self.check_list()?,
            FileSystemContract::CreateDirectory => self.check_create_directory()?,
            FileSystemContract::Representations => self.check_representations()?,
            FileSystemContract::Delete => {
                self.check_delete()?;
                self.check_delete_tree()?;
            }
            FileSystemContract::Copy => {
                self.check_copy()?;
            }
            FileSystemContract::Rename => {
                self.check_rename()?;
            }
            FileSystemContract::TempResources => self.check_temp_resources()?,
            FileSystemContract::ErrorContext => self.check_error_context()?,
        }
        Ok(())
    }
}
