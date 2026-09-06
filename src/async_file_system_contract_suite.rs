// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stateful runtime-neutral asynchronous filesystem provider contract suite.

use std::future::Future;
use std::panic::resume_unwind;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_fs::copy::AsyncCopyOperationState;
use qubit_fs::copy::CopyConflictPolicy;
use qubit_fs::copy::CopyFailureState;
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
use qubit_fs::temp::AsyncTempDirectory;
use qubit_fs::temp::AsyncTempFile;
use qubit_fs::temp::PersistFailureState;
use qubit_fs::temp::PersistOptions;
use qubit_fs::temp::PersistOutcome;
use qubit_fs::temp::TempOptions as TempDirectoryOptions;
use qubit_fs::temp::TempOptions as TempFileOptions;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteOptions;
use qubit_fs::write::WritePrecondition;
use qubit_io::AsyncOutput;

use crate::AsyncCopyCancellationStage;
use crate::AsyncFileSystemFixture;
use crate::FileSystemContract;
use crate::FixtureSupport;
use crate::contract_context::ContractContext;
use crate::internal::assert_error_with_source_or_target;
use crate::internal::assert_error_with_target;
use crate::internal::assert_unsupported_error;
use crate::internal::catch_unwind_future;
// Implements property snapshots and bounded limit checks.
mod properties;
// Implements reader contracts.
mod read;
// Implements writer and publication contracts.
mod write;
// Implements namespace and metadata contracts.
mod namespace;
// Implements copy and cancellation contracts.
mod copy;
// Implements temporary resource contracts.
mod temp;
// Implements fixture adaptation and suite lifecycle support.
mod support;

/// Runs asynchronous provider contracts against one isolated fixture.
///
/// # Type Parameters
///
/// * `'a` - Lifetime of the borrowed provider fixture.
#[must_use = "the suite must run at least one contract assertion"]
pub struct AsyncFileSystemContractSuite<'a> {
    /// Provider-owned fixture supplying the facade and observation hooks.
    fixture: &'a dyn AsyncFileSystemFixture,
    /// Property snapshot and cleanup state for the current suite run.
    context: ContractContext,
}

impl<'a> AsyncFileSystemContractSuite<'a> {
    /// Creates a stateful asynchronous suite borrowing one isolated fixture.
    ///
    /// # Parameters
    ///
    /// * `fixture` - Isolated provider fixture exercised by the suite.
    ///
    /// # Returns
    ///
    /// A suite with a fresh context and captured property snapshot.
    #[inline]
    pub fn new(fixture: &'a dyn AsyncFileSystemFixture) -> Self {
        Self {
            fixture,
            context: ContractContext::new(fixture.file_system().properties()),
        }
    }

    /// Runs all asynchronous contracts in their dependency-safe fixed order.
    ///
    /// # Panics
    ///
    /// Panics when the provider violates any contract or fixture setup and
    /// observation fails. Cleanup runs before an assertion panic is resumed.
    pub async fn assert_all(mut self) {
        let result = catch_unwind_future(async {
            for contract in FileSystemContract::ALL {
                self.assert_contract_inner(contract).await;
            }
        })
        .await;
        self.finish().await;
        if let Err(payload) = result {
            resume_unwind(payload);
        }
    }

    /// Runs one named asynchronous contract and always performs cleanup.
    ///
    /// # Panics
    ///
    /// Panics when the provider violates the selected contract or cleanup
    /// fails. Cleanup completes before an assertion panic is resumed.
    pub async fn assert_contract(mut self, contract: FileSystemContract) {
        let result = catch_unwind_future(async {
            self.assert_contract_inner(contract).await;
        })
        .await;
        self.finish().await;
        if let Err(payload) = result {
            resume_unwind(payload);
        }
    }

    /// Dispatches one named asynchronous phase without assuming a runtime.
    async fn assert_contract_inner(&mut self, contract: FileSystemContract) {
        match contract {
            FileSystemContract::Properties => self.assert_properties().await,
            FileSystemContract::Stat => self.assert_stat().await,
            FileSystemContract::Read => self.assert_read().await,
            FileSystemContract::Write => self.assert_write().await,
            FileSystemContract::List => self.assert_list().await,
            FileSystemContract::CreateDirectory => self.assert_create_directory().await,
            FileSystemContract::Representations => self.assert_representations().await,
            FileSystemContract::Delete => self.assert_delete().await,
            FileSystemContract::Copy => self.assert_copy().await,
            FileSystemContract::Rename => self.assert_rename().await,
            FileSystemContract::Append => self.assert_append().await,
            FileSystemContract::RecursiveDelete => self.assert_recursive_delete().await,
            FileSystemContract::AtomicRename => self.assert_atomic_rename().await,
            FileSystemContract::DurableRename => self.assert_durable_rename().await,
            FileSystemContract::AtomicReplace => self.assert_atomic_replace().await,
            FileSystemContract::DurableFileCopy => self.assert_durable_copy().await,
            FileSystemContract::TempResources => self.assert_temp_resources().await,
            FileSystemContract::ErrorContext => self.assert_error_context().await,
        }
    }
}
