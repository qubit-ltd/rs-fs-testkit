// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow all -- contract behavior is covered by the conforming and
// fault matrices.
//! Stateful runtime-neutral asynchronous filesystem provider contract suite.

use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

use qubit_fs::{
    AsyncCopyOperationState, CopyFailureState, CopyMethod, CreateDirectoryOptions, DeleteOptions,
    FileSystemCapability, FsErrorKind, FsOperation, RenameOptions, TempDirectoryOptions,
    TempFileOptions,
};
use qubit_io::{AsyncInput, AsyncOutput};

use crate::contract_context::ContractContext;
use crate::{AsyncCopyCancellationStage, AsyncFileSystemFixture, FixtureSupport};

/// Runs asynchronous provider contracts against one isolated fixture.
pub struct AsyncFileSystemContractSuite<'a> {
    fixture: &'a dyn AsyncFileSystemFixture,
    context: ContractContext,
}

impl<'a> AsyncFileSystemContractSuite<'a> {
    /// Creates a stateful asynchronous suite borrowing one isolated fixture.
    #[must_use]
    pub fn new(fixture: &'a dyn AsyncFileSystemFixture) -> Self {
        Self {
            fixture,
            context: ContractContext::new(fixture.file_system().properties()),
        }
    }

    /// Runs all asynchronous contracts in their dependency-safe fixed order.
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
        self.assert_temp_resources().await;
        self.assert_error_context().await;
        self.context.cleanup_async(self.fixture.file_system()).await;
    }

    /// Checks immutable facade properties and fixture path compatibility.
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
        assert_ne!(
            info.id().as_str(),
            info.provider_id(),
            "properties contract: filesystem id equals provider id"
        );
        let path = self
            .fixture
            .path("contract-properties")
            .unwrap_or_else(|error| panic!("properties contract: fixture path failed: {error}"));
        properties
            .path_constraints()
            .validate(&path)
            .unwrap_or_else(|error| {
                panic!("properties contract: fixture path violates constraints: {error}")
            });
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
    pub async fn assert_stat(&mut self) {
        self.context.begin("stat");
        let path = self.path("async-stat-missing");
        let error = self
            .fixture
            .file_system()
            .stat(&path)
            .await
            .expect_err("stat contract: missing path succeeded");
        self.assert_error(&error, FsErrorKind::NotFound, FsOperation::Stat, &path);
    }
    /// Checks asynchronous reader behavior.
    pub async fn assert_read(&mut self) {
        self.context.begin("read");
        if !self.capable(FileSystemCapability::Read) {
            let path = self.path("async-read-unavailable");
            let error = self
                .fixture
                .file_system()
                .open_reader(&path, Default::default())
                .await
                .expect_err("read contract: unadvertised reader open succeeded");
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
            .unwrap_or_else(|error| panic!("read contract: fixture seed failed: {error}"))
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
                panic!("read contract: advertised capability requires fixture.seed_file support")
            }
        }
    }
    /// Checks asynchronous writer behavior.
    pub async fn assert_write(&mut self) {
        self.context.begin("write");
        if !self.capable(FileSystemCapability::Write) {
            let path = self.path("async-write-unavailable");
            let error = self
                .fixture
                .file_system()
                .open_writer(&path, Default::default())
                .await
                .expect_err("write contract: unadvertised writer open succeeded");
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
            .unwrap_or_else(|error| panic!("write contract: fixture observation failed: {error}"))
        {
            FixtureSupport::Supported(bytes) => {
                assert_eq!(
                    bytes, b"async written",
                    "write contract: bytes were not published"
                )
            }
            FixtureSupport::Unsupported => {
                panic!("write contract: Write capability requires fixture.read_file support")
            }
        }
    }
    /// Checks asynchronous directory-listing behavior.
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
            assert_eq!(actual, expected, "list contract: direct children mismatch");
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
    pub async fn assert_create_directory(&mut self) {
        self.context.begin("create_directory");
        let path = self.path("async-created-directory");
        if self.capable(FileSystemCapability::CreateDirectory) {
            self.fixture
                .file_system()
                .create_directory(&path, CreateDirectoryOptions::default())
                .await
                .expect("create-directory contract: advertised creation failed");
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
            self.assert_error(&error, FsErrorKind::NotFound, FsOperation::Stat, &path);
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
    pub async fn assert_copy(&mut self) {
        self.context.begin("copy");
        if self.capable(FileSystemCapability::Copy) {
            let source = self
                .required_seed("async-copy-positive-source", b"copy bytes", "copy")
                .await;
            let target = self.path("async-copy-positive-target");
            let mut operation = self
                .fixture
                .file_system()
                .begin_copy(source.clone(), target.clone(), Default::default())
                .expect("async copy contract: advertised copy preflight failed");
            let outcome = operation.execute().await.unwrap_or_else(|failure| {
                panic!("async copy contract: copy failed: {}", failure.error())
            });
            match outcome.method() {
                CopyMethod::Streamed => assert!(
                    outcome.used_fallback(),
                    "async copy contract: streamed copy was not reported as fallback"
                ),
                CopyMethod::Native
                | CopyMethod::Clone
                | CopyMethod::ServerSide
                | CopyMethod::Mixed => assert!(
                    !outcome.used_fallback(),
                    "async copy contract: completed fast path was reported as fallback"
                ),
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
        }
        for stage in [
            AsyncCopyCancellationStage::NativeAttempt,
            AsyncCopyCancellationStage::Reader,
            AsyncCopyCancellationStage::Writer,
            AsyncCopyCancellationStage::Commit,
        ] {
            let case = self
                .fixture
                .copy_cancellation_case(stage)
                .unwrap_or_else(|error| {
                    panic!("async copy contract: fixture setup failed: {error}")
                });
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
                AsyncCopyOperationState::Failed(CopyFailureState::Indeterminate),
                "async copy contract: cancellation did not become indeterminate"
            );
            if matches!(
                stage,
                AsyncCopyCancellationStage::Writer | AsyncCopyCancellationStage::Commit
            ) {
                assert!(
                    operation.take_recovery_writer().is_some(),
                    "async copy contract: recovery writer was lost"
                );
            }
        }
    }
    /// Checks asynchronous rename behavior.
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
                .unwrap_or_else(|failure| {
                    panic!(
                        "rename contract: advertised rename failed: {}",
                        failure.error()
                    )
                });
            let error = self
                .fixture
                .file_system()
                .stat(&source)
                .await
                .expect_err("rename contract: source remained after success");
            self.assert_error(&error, FsErrorKind::NotFound, FsOperation::Stat, &source);
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
    /// Checks asynchronous temporary-resource lifecycle behavior.
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
            self.assert_error(&error, FsErrorKind::NotFound, FsOperation::Stat, &path);
        } else {
            let error = match self
                .fixture
                .file_system()
                .create_temp_file(TempFileOptions::default())
                .await
            {
                Ok(_) => panic!("temp-file contract: unadvertised creation succeeded"),
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
            let error = self
                .fixture
                .file_system()
                .stat(&path)
                .await
                .expect_err("temp-directory contract: cleanup retained source");
            self.assert_error(&error, FsErrorKind::NotFound, FsOperation::Stat, &path);
        } else {
            let error = match self
                .fixture
                .file_system()
                .create_temp_directory(TempDirectoryOptions::default())
                .await
            {
                Ok(_) => panic!("temp-directory contract: unadvertised creation succeeded"),
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
    pub async fn assert_error_context(&mut self) {
        self.context.begin("error_context");
        let path = self.path("async-error-context-missing");
        let error = self
            .fixture
            .file_system()
            .stat(&path)
            .await
            .expect_err("error contract: missing path succeeded");
        self.assert_error(&error, FsErrorKind::NotFound, FsOperation::Stat, &path);
    }

    /// Resolves a fixture path or identifies setup failure at the contract
    /// boundary.
    fn path(&self, relative: &str) -> qubit_fs::Path {
        let relative = self.context.relative_name(relative);
        self.fixture.path(&relative).unwrap_or_else(|error| {
            panic!(
                "{} contract: fixture path failed: {error}",
                self.context.current_contract()
            )
        })
    }

    /// Returns whether the cached property snapshot advertises a capability.
    fn capable(&self, capability: FileSystemCapability) -> bool {
        self.context
            .properties()
            .capabilities()
            .contains(capability)
    }

    /// Seeds a resource and makes fixture support mandatory for the contract.
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
            .unwrap_or_else(|error| panic!("{contract} contract: fixture seed failed: {error}"))
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
    async fn assert_bytes(&self, path: &qubit_fs::Path, expected: &[u8], message: &str) {
        match self.fixture.read_file(path).await.unwrap_or_else(|error| {
            panic!("async copy contract: fixture observation failed: {error}")
        }) {
            FixtureSupport::Supported(actual) => {
                assert_eq!(actual, expected, "{message}")
            }
            FixtureSupport::Unsupported => {
                panic!("async copy contract: Copy capability requires fixture.read_file support")
            }
        }
    }

    /// Checks public context on an asynchronous facade error.
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
}
