// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stateful runtime-neutral asynchronous filesystem provider contract suite.

use std::{
    future::Future,
    task::{
        Context,
        Poll,
        Waker,
    },
};

use qubit_fs::{
    AchievedAtomicity,
    AsyncCopyOperationState,
    AtomicityRequirement,
    CopyFailureState,
    CopyMethod,
    CopyOptions,
    CreateDirectoryOptions,
    DeleteOptions,
    DurabilityRequirement,
    FileSystemCapability,
    FsErrorKind,
    FsOperation,
    RenameOptions,
    TempDirectoryOptions,
    TempFileOptions,
    WriteDisposition,
    WriteOptions,
};
use qubit_io::{
    AsyncInput,
    AsyncOutput,
};

use crate::contract_context::ContractContext;
use crate::{
    AsyncCopyCancellationStage,
    AsyncFileSystemFixture,
    FixtureSupport,
};

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
    /// observation fails. Cleanup runs after all phases complete normally.
    pub async fn assert_all(mut self) {
        self.assert_properties().await;
        self.assert_stat().await;
        self.assert_read().await;
        self.assert_write().await;
        self.assert_list().await;
        self.assert_create_directory().await;
        self.assert_delete().await;
        self.assert_copy().await;
        self.assert_rename().await;
        self.assert_append().await;
        self.assert_recursive_delete().await;
        self.assert_atomic_rename().await;
        self.assert_atomic_replace().await;
        self.assert_durable_copy().await;
        self.assert_temp_resources().await;
        self.assert_error_context().await;
        self.finish().await;
    }

    /// Cleans resources created by individually executed asynchronous phases.
    ///
    /// # Panics
    ///
    /// Panics when a recorded resource cannot be inspected or deleted.
    pub async fn finish(&mut self) {
        self.context.cleanup_async(self.fixture.file_system()).await;
    }

    /// Checks immutable facade properties and fixture path compatibility.
    ///
    /// # Panics
    ///
    /// Panics when identifiers or capabilities are inconsistent, the fixture
    /// path is invalid, or the facade snapshot changes during the suite run.
    pub async fn assert_properties(&mut self) {
        self.context.begin("properties");
        let properties = self.context.properties();
        let info = properties.info();
        assert!(
            !info.id().as_str().is_empty(),
            "properties contract: filesystem id is empty"
        );
        assert!(
            !info.provider_id().is_empty(),
            "properties contract: provider id is empty"
        );
        let path = self
            .fixture
            .path("contract-properties")
            .expect("properties contract: fixture path failed");
        properties
            .path_constraints()
            .validate(&path)
            .expect("properties contract: fixture path violates constraints");
        assert_eq!(
            properties.info(),
            self.fixture.file_system().properties().info(),
            "properties contract: snapshot changed"
        );
        assert_eq!(
            properties.capabilities(),
            self.fixture.file_system().properties().capabilities(),
            "properties contract: snapshot changed"
        );
    }

    /// Checks asynchronous metadata behavior.
    ///
    /// # Panics
    ///
    /// Panics when missing-path errors omit or misreport structured context.
    pub async fn assert_stat(&mut self) {
        self.context.begin("stat");
        let path = self.path("async-stat-missing");
        let error = self
            .fixture
            .file_system()
            .stat(&path)
            .await
            .expect_err("stat contract: missing path succeeded");
        self.assert_error(
            &error,
            FsErrorKind::NotFound,
            FsOperation::Stat,
            &path,
        );
    }

    /// Checks asynchronous reader behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, seeded reads, byte limits, or
    /// structured error context violates the reader contract.
    pub async fn assert_read(&mut self) {
        self.context.begin("read");
        if !self.capable(FileSystemCapability::Read) {
            let path = self.path("async-read-unavailable");
            let error = self
                .fixture
                .file_system()
                .open_reader(&path, Default::default())
                .await
                .expect_err(
                    "read contract: unadvertised reader open succeeded",
                );
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::OpenReader,
                &path,
            );
            assert_eq!(
                error.required_capability(),
                Some(FileSystemCapability::Read),
                "read contract: missing required-capability context"
            );
            return;
        }
        match self
            .fixture
            .seed_file("async-read", b"async bytes")
            .await
            .expect("read contract: fixture seed failed")
        {
            FixtureSupport::Supported(path) => {
                self.context.record_created(path.clone());
                let mut reader = self
                    .fixture
                    .file_system()
                    .open_reader(&path, Default::default())
                    .await
                    .expect("read contract: seeded path is not readable");
                let mut actual = [0; 11];
                reader
                    .read_exactly_async(&mut actual)
                    .await
                    .expect("read contract: seeded bytes could not be read");
                assert_eq!(
                    actual, *b"async bytes",
                    "read contract: seeded bytes mismatch"
                );
            }
            FixtureSupport::Unsupported => {
                panic!(
                    "read contract: advertised capability requires fixture.seed_file support"
                )
            }
        }
    }

    /// Checks asynchronous writer behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, publication, fixture observation, or
    /// structured error context violates the writer contract.
    pub async fn assert_write(&mut self) {
        self.context.begin("write");
        if !self.capable(FileSystemCapability::Write) {
            let path = self.path("async-write-unavailable");
            let error = self
                .fixture
                .file_system()
                .open_writer(&path, Default::default())
                .await
                .expect_err(
                    "write contract: unadvertised writer open succeeded",
                );
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::OpenWriter,
                &path,
            );
            assert_eq!(
                error.required_capability(),
                Some(FileSystemCapability::Write),
                "write contract: missing required-capability context"
            );
            return;
        }
        let path = self.path("async-write");
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&path, Default::default())
            .await
            .expect("write contract: writer open failed");
        writer
            .write_fully_async(b"async written")
            .await
            .expect("write contract: writer rejected bytes");
        writer
            .commit_async()
            .await
            .expect("write contract: writer commit failed");
        self.context.record_created(path.clone());
        match self
            .fixture
            .read_file(&path)
            .await
            .expect("write contract: fixture observation failed")
        {
            FixtureSupport::Supported(bytes) => {
                assert_eq!(
                    bytes, b"async written",
                    "write contract: bytes were not published"
                )
            }
            FixtureSupport::Unsupported => {
                panic!(
                    "write contract: Write capability requires fixture.read_file support"
                )
            }
        }
    }

    /// Checks asynchronous directory-listing behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, child enumeration, prefix filtering,
    /// pagination, or requested metadata violates the listing contract.
    pub async fn assert_list(&mut self) {
        self.context.begin("list");
        let path = self.path("async-list");
        if self.capable(FileSystemCapability::List) {
            let first = self
                .required_seed("async-list/first", b"first", "list")
                .await;
            let second = self
                .required_seed("async-list/second", b"second", "list")
                .await;
            let mut stream = self
                .fixture
                .file_system()
                .list(&path, Default::default())
                .await
                .expect("list contract: advertised listing failed");
            let mut actual = Vec::new();
            while let Some(entry) = stream
                .next_entry_async()
                .await
                .expect("list contract: stream error")
            {
                actual.push(entry.path);
            }
            actual.sort_by(|left, right| left.as_str().cmp(right.as_str()));
            let mut expected = vec![first, second];
            expected.sort_by(|left, right| left.as_str().cmp(right.as_str()));
            assert_eq!(
                actual, expected,
                "list contract: direct children mismatch"
            );
        } else {
            let error = self
                .fixture
                .file_system()
                .list(&path, Default::default())
                .await
                .expect_err("list contract: unadvertised listing succeeded");
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::List,
                &path,
            );
            assert_eq!(
                error.required_capability(),
                Some(FileSystemCapability::List),
                "list contract: missing required-capability context"
            );
        }
    }

    /// Checks asynchronous directory-creation behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, directory publication, metadata, or
    /// existing-directory handling violates the creation contract.
    pub async fn assert_create_directory(&mut self) {
        self.context.begin("create_directory");
        let path = self.path("async-created-directory");
        if self.capable(FileSystemCapability::CreateDirectory) {
            self.fixture
                .file_system()
                .create_directory(&path, CreateDirectoryOptions::default())
                .await
                .expect(
                    "create-directory contract: advertised creation failed",
                );
            self.context.record_created(path.clone());
            let metadata = self
                .fixture
                .file_system()
                .stat(&path)
                .await
                .expect("create-directory contract: created path missing");
            assert!(
                metadata.is_directory_like(),
                "create-directory contract: created path is not a directory"
            );
        } else {
            let error = self
                .fixture
                .file_system()
                .create_directory(&path, CreateDirectoryOptions::default())
                .await
                .expect_err("create-directory contract: unadvertised creation succeeded");
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::CreateDir,
                &path,
            );
        }
    }

    /// Checks asynchronous deletion behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, deletion, existence observation, or
    /// structured error context violates the deletion contract.
    pub async fn assert_delete(&mut self) {
        self.context.begin("delete");
        let path = self.path("async-delete");
        if self.capable(FileSystemCapability::Delete) {
            let path = self
                .required_seed("async-delete", b"delete", "delete")
                .await;
            self.fixture
                .file_system()
                .delete_file(&path, DeleteOptions::default())
                .await
                .expect("delete contract: advertised deletion failed");
            let error = self
                .fixture
                .file_system()
                .stat(&path)
                .await
                .expect_err("delete contract: deleted file remained");
            self.assert_error(
                &error,
                FsErrorKind::NotFound,
                FsOperation::Stat,
                &path,
            );
        } else {
            let error = self
                .fixture
                .file_system()
                .delete_file(&path, DeleteOptions::default())
                .await
                .expect_err("delete contract: unadvertised delete succeeded");
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::Delete,
                &path,
            );
        }
    }

    /// Checks asynchronous native and fallback copy behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, copy publication, cancellation,
    /// method reporting, or directory recursion violates the copy contract.
    pub async fn assert_copy(&mut self) {
        self.context.begin("copy");
        if !self.capable(FileSystemCapability::Copy) {
            let source = self.path("async-copy-unavailable");
            let error = match self.fixture.file_system().begin_copy(
                source.clone(),
                source.clone(),
                Default::default(),
            ) {
                Err(error) => error,
                Ok(_) => panic!(
                    "async copy contract: unadvertised copy preflight succeeded"
                ),
            };
            self.assert_error(
                error.error(),
                FsErrorKind::UnsupportedCapability,
                FsOperation::Copy,
                &source,
            );
            assert_eq!(
                error.error().required_capability(),
                Some(FileSystemCapability::Copy),
                "async copy contract: missing required-capability context"
            );
            return;
        }
        let source = self
            .required_seed("async-copy-positive-source", b"copy bytes", "copy")
            .await;
        let target = self.path("async-copy-positive-target");
        let mut operation = self
            .fixture
            .file_system()
            .begin_copy(source.clone(), target.clone(), Default::default())
            .expect("async copy contract: advertised copy preflight failed");
        let outcome = operation
            .execute()
            .await
            .expect("async copy contract: copy failed");
        match outcome.method() {
            CopyMethod::Streamed => assert!(
                outcome.used_fallback(),
                "async copy contract: streamed copy was not reported as fallback"
            ),
            CopyMethod::Native
            | CopyMethod::Clone
            | CopyMethod::ServerSide
            | CopyMethod::Mixed => {
                assert!(
                    !outcome.used_fallback(),
                    "async copy contract: completed fast path was reported as fallback"
                )
            }
        }
        self.assert_bytes(
            &source,
            b"copy bytes",
            "async copy contract: source was modified",
        )
        .await;
        self.assert_bytes(
            &target,
            b"copy bytes",
            "async copy contract: target bytes mismatch",
        )
        .await;
        self.context.record_created(target);
        for stage in [
            AsyncCopyCancellationStage::NativeAttempt,
            AsyncCopyCancellationStage::Reader,
            AsyncCopyCancellationStage::Writer,
            AsyncCopyCancellationStage::Commit,
        ] {
            let case = self
                .fixture
                .copy_cancellation_case(stage)
                .expect("async copy contract: fixture setup failed");
            let case = match case {
                FixtureSupport::Supported(case) => case,
                FixtureSupport::Unsupported => continue,
            };
            self.context.record_created(case.source().clone());
            self.context.record_created(case.target().clone());
            let mut operation = self
                .fixture
                .file_system()
                .begin_copy(
                    case.source().clone(),
                    case.target().clone(),
                    case.options().clone(),
                )
                .expect("async copy contract: preflight failed");
            let mut execution = Box::pin(operation.execute());
            let waker = Waker::noop();
            let mut task = Context::from_waker(waker);
            assert!(
                matches!(execution.as_mut().poll(&mut task), Poll::Pending),
                "async copy contract: fixture stage {stage:?} did not pend"
            );
            drop(execution);
            assert_eq!(
                operation.state(),
                AsyncCopyOperationState::Failed(
                    CopyFailureState::Indeterminate
                ),
                "async copy contract: cancellation did not become indeterminate"
            );
            if matches!(
                stage,
                AsyncCopyCancellationStage::Writer
                    | AsyncCopyCancellationStage::Commit
            ) {
                assert!(
                    operation.take_recovery_writer().is_some(),
                    "async copy contract: recovery writer was lost"
                );
            }
        }
    }

    /// Checks asynchronous rename behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, publication, source removal, target
    /// content, or structured error context violates the rename contract.
    pub async fn assert_rename(&mut self) {
        self.context.begin("rename");
        let source = self.path("async-rename-source");
        let target = self.path("async-rename-target");
        if self.capable(FileSystemCapability::Rename) {
            let source = self
                .required_seed("async-rename-source", b"rename", "rename")
                .await;
            let target = self.path("async-rename-target");
            self.fixture
                .file_system()
                .rename(&source, &target, RenameOptions::default())
                .await
                .expect("rename contract: advertised rename failed");
            let error =
                self.fixture.file_system().stat(&source).await.expect_err(
                    "rename contract: source remained after success",
                );
            self.assert_error(
                &error,
                FsErrorKind::NotFound,
                FsOperation::Stat,
                &source,
            );
            self.fixture
                .file_system()
                .stat(&target)
                .await
                .expect("rename contract: target missing after success");
            self.context.record_created(target);
        } else {
            let failure = self
                .fixture
                .file_system()
                .rename(&source, &target, RenameOptions::default())
                .await
                .expect_err("rename contract: unadvertised rename succeeded");
            self.assert_error(
                failure.error(),
                FsErrorKind::UnsupportedCapability,
                FsOperation::Rename,
                &source,
            );
        }
    }

    /// Checks asynchronous append writes when the provider advertises them.
    ///
    /// # Panics
    ///
    /// Panics when append preflight or publication violates the advertised
    /// capability, or fixture setup and observation fail.
    pub async fn assert_append(&mut self) {
        self.context.begin("append");
        let path = self.path("async-append-target");
        let options = WriteOptions {
            disposition: WriteDisposition::Append,
            ..WriteOptions::default()
        };
        if !self.capable(FileSystemCapability::Append) {
            let error = self
                .fixture
                .file_system()
                .open_writer(&path, options)
                .await
                .expect_err(
                    "append contract: unadvertised append preflight succeeded",
                );
            self.assert_requirement_error(
                &error,
                FsOperation::OpenWriter,
                FileSystemCapability::Append,
                "append contract",
            );
            return;
        }
        let path = self
            .required_seed("async-append-target", b"before", "append")
            .await;
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&path, options)
            .await
            .expect("append contract: writer open failed");
        writer
            .write_fully_async(b"-after")
            .await
            .expect("append contract: write failed");
        writer
            .commit_async()
            .await
            .expect("append contract: commit failed");
        self.assert_bytes(
            &path,
            b"before-after",
            "append contract: existing bytes were not retained",
        )
        .await;
    }

    /// Checks asynchronous recursive deletion when the provider advertises it.
    ///
    /// # Panics
    ///
    /// Panics when recursive-delete preflight or descendant removal violates
    /// the advertised capability, or fixture setup fails.
    pub async fn assert_recursive_delete(&mut self) {
        self.context.begin("recursive_delete");
        let root = self.path("async-recursive-delete-root");
        let options = DeleteOptions {
            recursive: true,
            ..DeleteOptions::default()
        };
        if !self.capable(FileSystemCapability::RecursiveDelete) {
            let error = self
                .fixture
                .file_system()
                .delete_directory(&root, options)
                .await
                .expect_err("recursive-delete contract: unadvertised preflight succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::Delete,
                FileSystemCapability::RecursiveDelete,
                "recursive-delete contract",
            );
            return;
        }
        self.fixture
            .file_system()
            .create_directory(&root, CreateDirectoryOptions::default())
            .await
            .expect("recursive-delete contract: root creation failed");
        let child = self
            .required_seed(
                "async-recursive-delete-root/child",
                b"child",
                "recursive-delete",
            )
            .await;
        self.fixture
            .file_system()
            .delete_directory(&root, options)
            .await
            .expect("recursive-delete contract: recursive removal failed");
        assert!(
            !self.fixture.file_system().exists(&root).await.expect(
                "recursive-delete contract: root existence check failed"
            )
        );
        assert!(
            !self.fixture.file_system().exists(&child).await.expect(
                "recursive-delete contract: child existence check failed"
            )
        );
    }

    /// Checks asynchronous atomic rename publication when advertised.
    ///
    /// # Panics
    ///
    /// Panics when atomic-rename preflight, publication, or reported atomicity
    /// violates the advertised capability.
    pub async fn assert_atomic_rename(&mut self) {
        self.context.begin("atomic_rename");
        let source = self.path("async-atomic-rename-source");
        let target = self.path("async-atomic-rename-target");
        let options = RenameOptions {
            atomicity: AtomicityRequirement::Required,
            ..RenameOptions::default()
        };
        if !self.capable(FileSystemCapability::AtomicRename) {
            let failure = self
                .fixture
                .file_system()
                .rename(&source, &target, options)
                .await
                .expect_err(
                    "atomic-rename contract: unadvertised preflight succeeded",
                );
            self.assert_requirement_error(
                failure.error(),
                FsOperation::Rename,
                FileSystemCapability::AtomicRename,
                "atomic-rename contract",
            );
            return;
        }
        let source = self
            .required_seed(
                "async-atomic-rename-source",
                b"atomic rename",
                "atomic-rename",
            )
            .await;
        let outcome = self
            .fixture
            .file_system()
            .rename(&source, &target, options)
            .await
            .expect("atomic-rename contract: required rename failed");
        assert_eq!(
            outcome.atomicity(),
            AchievedAtomicity::Atomic,
            "atomic-rename contract: non-atomic outcome"
        );
        self.context.record_created(target);
    }

    /// Checks asynchronous atomic replacement publication when advertised.
    ///
    /// # Panics
    ///
    /// Panics when atomic-replacement preflight, publication, or reported
    /// atomicity violates the advertised capability.
    pub async fn assert_atomic_replace(&mut self) {
        self.context.begin("atomic_replace");
        let path = self.path("async-atomic-replace-target");
        let options = WriteOptions {
            atomicity: AtomicityRequirement::Required,
            ..WriteOptions::default()
        };
        if !self.capable(FileSystemCapability::AtomicReplace) {
            let error = self
                .fixture
                .file_system()
                .open_writer(&path, options)
                .await
                .expect_err(
                    "atomic-replace contract: unadvertised preflight succeeded",
                );
            self.assert_requirement_error(
                &error,
                FsOperation::OpenWriter,
                FileSystemCapability::AtomicReplace,
                "atomic-replace contract",
            );
            return;
        }
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&path, options)
            .await
            .expect("atomic-replace contract: writer open failed");
        writer
            .write_fully_async(b"atomic replacement")
            .await
            .expect("atomic-replace contract: write failed");
        let outcome = writer
            .commit_async()
            .await
            .expect("atomic-replace contract: commit failed");
        assert_eq!(
            outcome.atomicity,
            AchievedAtomicity::Atomic,
            "atomic-replace contract: non-atomic outcome"
        );
        self.context.record_created(path);
    }

    /// Checks asynchronous durable copy publication when advertised.
    ///
    /// # Panics
    ///
    /// Panics when durable-copy preflight, publication, reported durability,
    /// or target content violates the advertised capability.
    pub async fn assert_durable_copy(&mut self) {
        self.context.begin("durable_copy");
        let source = self.path("async-durable-copy-source");
        let target = self.path("async-durable-copy-target");
        let options = CopyOptions {
            durability: DurabilityRequirement::Required,
            ..CopyOptions::default()
        };
        if !self.capable(FileSystemCapability::DurableCopy) {
            let error = match self
                .fixture
                .file_system()
                .begin_copy(source, target, options)
            {
                Err(error) => error,
                Ok(_) => panic!(
                    "durable-copy contract: unadvertised preflight succeeded"
                ),
            };
            self.assert_requirement_error(
                error.error(),
                FsOperation::Copy,
                FileSystemCapability::DurableCopy,
                "durable-copy contract",
            );
            return;
        }
        let source = self
            .required_seed(
                "async-durable-copy-source",
                b"durable copy",
                "durable-copy",
            )
            .await;
        let mut operation = self
            .fixture
            .file_system()
            .begin_copy(source.clone(), target.clone(), options)
            .expect("durable-copy contract: preflight failed");
        let outcome = operation
            .execute()
            .await
            .expect("durable-copy contract: copy failed");
        assert!(
            outcome.durable(),
            "durable-copy contract: non-durable outcome"
        );
        self.assert_bytes(
            &target,
            b"durable copy",
            "durable-copy contract: target bytes mismatch",
        )
        .await;
        self.context.record_created(source);
        self.context.record_created(target);
    }

    /// Checks asynchronous temporary-resource lifecycle behavior.
    ///
    /// # Panics
    ///
    /// Panics when temporary resource options, cleanup, or publication violates
    /// an advertised capability.
    pub async fn assert_temp_resources(&mut self) {
        self.context.begin("temp_resources");
        if self.capable(FileSystemCapability::TempFile) {
            let mut temporary = self
                .fixture
                .file_system()
                .create_temp_file(TempFileOptions::default())
                .await
                .expect("temp-file contract: advertised creation failed");
            let path = temporary.path().clone();
            temporary
                .cleanup()
                .await
                .expect("temp-file contract: cleanup failed");
            let error = self
                .fixture
                .file_system()
                .stat(&path)
                .await
                .expect_err("temp-file contract: cleanup retained source");
            self.assert_error(
                &error,
                FsErrorKind::NotFound,
                FsOperation::Stat,
                &path,
            );
        } else {
            let error = match self
                .fixture
                .file_system()
                .create_temp_file(TempFileOptions::default())
                .await
            {
                Ok(_) => panic!(
                    "temp-file contract: unadvertised creation succeeded"
                ),
                Err(error) => error,
            };
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::CreateTemp,
                &qubit_fs::Path::root(),
            );
        }
        if self.capable(FileSystemCapability::TempDirectory) {
            let mut temporary = self
                .fixture
                .file_system()
                .create_temp_directory(TempDirectoryOptions::default())
                .await
                .expect("temp-directory contract: advertised creation failed");
            let path = temporary.path().clone();
            temporary
                .cleanup()
                .await
                .expect("temp-directory contract: cleanup failed");
            let error =
                self.fixture.file_system().stat(&path).await.expect_err(
                    "temp-directory contract: cleanup retained source",
                );
            self.assert_error(
                &error,
                FsErrorKind::NotFound,
                FsOperation::Stat,
                &path,
            );
        } else {
            let error = match self
                .fixture
                .file_system()
                .create_temp_directory(TempDirectoryOptions::default())
                .await
            {
                Ok(_) => panic!(
                    "temp-directory contract: unadvertised creation succeeded"
                ),
                Err(error) => error,
            };
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::CreateTemp,
                &qubit_fs::Path::root(),
            );
        }
    }

    /// Checks asynchronous structured-error context and redaction behavior.
    ///
    /// # Panics
    ///
    /// Panics when a missing-path error omits or misreports its structured
    /// kind, operation, or path context.
    pub async fn assert_error_context(&mut self) {
        self.context.begin("error_context");
        let path = self.path("async-error-context-missing");
        let error = self
            .fixture
            .file_system()
            .stat(&path)
            .await
            .expect_err("error contract: missing path succeeded");
        self.assert_error(
            &error,
            FsErrorKind::NotFound,
            FsOperation::Stat,
            &path,
        );
    }

    /// Resolves a fixture path or identifies setup failure at the contract
    /// boundary.
    ///
    /// # Parameters
    ///
    /// * `relative` - Resource name relative to the current contract phase.
    ///
    /// # Returns
    ///
    /// The provider path mapped by the fixture.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot map the generated relative name.
    #[inline]
    fn path(&self, relative: &str) -> qubit_fs::Path {
        let relative = self.context.relative_name(relative);
        self.fixture
            .path(&relative)
            .expect("async contract: fixture path failed")
    }

    /// Returns whether the cached property snapshot advertises a capability.
    ///
    /// # Parameters
    ///
    /// * `capability` - Capability to query in the captured snapshot.
    ///
    /// # Returns
    ///
    /// `true` when the provider advertises the capability.
    #[inline(always)]
    fn capable(&self, capability: FileSystemCapability) -> bool {
        self.context
            .properties()
            .capabilities()
            .contains(capability)
    }

    /// Seeds a resource and makes fixture support mandatory for the contract.
    ///
    /// # Parameters
    ///
    /// * `relative` - Resource name relative to the current contract phase.
    /// * `bytes` - Exact content to publish.
    /// * `contract` - Contract label used in failure diagnostics.
    ///
    /// # Returns
    ///
    /// The provider path of the seeded file.
    ///
    /// # Panics
    ///
    /// Panics when fixture setup fails or the required seed hook is
    /// unsupported.
    async fn required_seed(
        &mut self,
        relative: &str,
        bytes: &[u8],
        contract: &str,
    ) -> qubit_fs::Path {
        let relative = self.context.relative_name(relative);
        match self
            .fixture
            .seed_file(&relative, bytes)
            .await
            .expect("async contract: fixture seed failed")
        {
            FixtureSupport::Supported(path) => {
                self.context.record_created(path.clone());
                path
            }
            FixtureSupport::Unsupported => {
                panic!(
                    "{contract} contract: advertised capability requires fixture.seed_file support"
                )
            }
        }
    }

    /// Reads a fixture-owned file and checks its exact bytes after copy.
    ///
    /// # Parameters
    ///
    /// * `path` - Provider path to observe.
    /// * `expected` - Exact expected content.
    /// * `message` - Assertion message used when content differs.
    ///
    /// # Panics
    ///
    /// Panics when observation fails, is unsupported, or returns different
    /// content.
    async fn assert_bytes(
        &self,
        path: &qubit_fs::Path,
        expected: &[u8],
        message: &str,
    ) {
        match self
            .fixture
            .read_file(path)
            .await
            .expect("async copy contract: fixture observation failed")
        {
            FixtureSupport::Supported(actual) => {
                assert_eq!(actual, expected, "{message}")
            }
            FixtureSupport::Unsupported => {
                panic!(
                    "async copy contract: Copy capability requires fixture.read_file support"
                )
            }
        }
    }

    /// Checks public context on an asynchronous facade error.
    ///
    /// # Parameters
    ///
    /// * `error` - Actual filesystem error.
    /// * `kind` - Expected error classification.
    /// * `operation` - Expected public operation.
    /// * `path` - Expected source path.
    ///
    /// # Panics
    ///
    /// Panics when any expected structured field differs.
    fn assert_error(
        &self,
        error: &qubit_fs::FsError,
        kind: FsErrorKind,
        operation: FsOperation,
        path: &qubit_fs::Path,
    ) {
        assert_eq!(error.kind(), kind, "error contract: kind mismatch");
        assert_eq!(
            error.operation(),
            operation,
            "error contract: context mismatch"
        );
        assert_eq!(error.path(), Some(path), "error contract: path mismatch");
    }

    /// Validates option-derived asynchronous capability preflight errors.
    ///
    /// # Parameters
    ///
    /// * `error` - Actual filesystem error.
    /// * `operation` - Expected public operation.
    /// * `capability` - Capability required by the rejected options.
    /// * `contract` - Contract label used in diagnostics.
    ///
    /// # Panics
    ///
    /// Panics when the error kind, operation, or required capability differs.
    fn assert_requirement_error(
        &self,
        error: &qubit_fs::FsError,
        operation: FsOperation,
        capability: FileSystemCapability,
        contract: &str,
    ) {
        assert_eq!(
            error.kind(),
            FsErrorKind::RequirementNotMet,
            "{contract}: error kind mismatch"
        );
        assert_eq!(
            error.operation(),
            operation,
            "{contract}: error operation mismatch"
        );
        assert_eq!(
            error.required_capability(),
            Some(capability),
            "{contract}: missing required-capability context"
        );
    }
}
