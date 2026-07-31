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
    panic::resume_unwind,
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
    ChecksumPolicy,
    CopyConflictPolicy,
    CopyFailureState,
    CopyMethod,
    CopyOptions,
    CreateDirectoryOptions,
    DeleteOptions,
    DurabilityRequirement,
    FileKind,
    FileSystemCapability,
    FsErrorKind,
    FsOperation,
    ListOptions,
    PersistFailureState,
    PersistOptions,
    ReadOptions,
    RenameOptions,
    RenameFailureState,
    ResourceVersion,
    ServerSidePreference,
    TempDirectoryOptions,
    TempFileOptions,
    WriteDisposition,
    WriteOptions,
    WritePrecondition,
};
use qubit_io::AsyncOutput;

use crate::contract_context::ContractContext;
use crate::internal::{
    assert_error_with_target,
    assert_unsupported_error,
    catch_unwind_future,
};
use crate::{
    FileSystemContract,
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
            FileSystemContract::CreateDirectory => {
                self.assert_create_directory().await
            }
            FileSystemContract::Representations => {
                self.assert_representations().await
            }
            FileSystemContract::Delete => self.assert_delete().await,
            FileSystemContract::Copy => self.assert_copy().await,
            FileSystemContract::Rename => self.assert_rename().await,
            FileSystemContract::Append => self.assert_append().await,
            FileSystemContract::RecursiveDelete => {
                self.assert_recursive_delete().await
            }
            FileSystemContract::AtomicRename => {
                self.assert_atomic_rename().await
            }
            FileSystemContract::AtomicReplace => {
                self.assert_atomic_replace().await
            }
            FileSystemContract::DurableCopy => self.assert_durable_copy().await,
            FileSystemContract::TempResources => {
                self.assert_temp_resources().await
            }
            FileSystemContract::ErrorContext => {
                self.assert_error_context().await
            }
        }
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
        assert!(
            properties.capabilities().missing_dependency().is_none(),
            "properties contract: capability dependencies are inconsistent"
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
        if let FixtureSupport::Supported(path) = self
            .fixture
            .seed_file("async-stat-file", b"stateful stat")
            .await
            .expect("stat contract: fixture seed failed")
        {
            self.context.record_created(path.clone());
            let metadata = self
                .fixture
                .file_system()
                .stat(&path)
                .await
                .expect("stat contract: seeded file is not statable");
            assert!(
                metadata.is_file_like(),
                "stat contract: seeded resource is not file-like"
            );
            assert_eq!(
                metadata.len,
                Some(13),
                "stat contract: file length mismatch"
            );
        }
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
        let path = match self
            .fixture
            .seed_file("async-read", b"async bytes")
            .await
            .expect("read contract: fixture seed failed")
        {
            FixtureSupport::Supported(path) => {
                self.context.record_created(path.clone());
                let actual = self
                    .fixture
                    .file_system()
                    .read_all(&path, Default::default(), 64)
                    .await
                    .expect("read contract: facade could not read seeded bytes");
                assert_eq!(
                    actual, b"async bytes",
                    "read contract: seeded bytes mismatch"
                );
                let error = self
                    .fixture
                    .file_system()
                    .read_all(&path, Default::default(), 4)
                    .await
                    .expect_err("read contract: caller byte limit was ignored");
                self.assert_error(
                    &error,
                    FsErrorKind::ResourceLimitExceeded,
                    FsOperation::Read,
                    &path,
                );
                path
            }
            FixtureSupport::Unsupported => {
                panic!(
                    "read contract: advertised capability requires fixture.seed_file support"
                )
            }
        };
        self.assert_read_options(&path).await;
    }

    /// Checks asynchronous range, conditional, and checksum read guarantees.
    async fn assert_read_options(&self, path: &qubit_fs::Path) {
        let range = ReadOptions {
            offset: Some(6),
            length: Some(5),
            ..ReadOptions::default()
        };
        if self.capable(FileSystemCapability::RangeRead) {
            let bytes = self
                .fixture
                .file_system()
                .read_all(path, range, 64)
                .await
                .expect("read contract: advertised range read failed");
            assert_eq!(bytes, b"bytes", "read contract: range mismatch");
        } else {
            let error = self
                .fixture
                .file_system()
                .open_reader(path, range)
                .await
                .expect_err("read contract: unadvertised range read succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::OpenReader,
                FileSystemCapability::RangeRead,
                "range-read contract",
            );
        }

        let version_support = self
            .fixture
            .resource_version(path)
            .await
            .expect("read contract: version observation failed");
        let conditional = ReadOptions {
            if_match: Some(match &version_support {
                FixtureSupport::Supported(version) => version.clone(),
                FixtureSupport::Unsupported => {
                    ResourceVersion::new("contract-version")
                }
            }),
            ..ReadOptions::default()
        };
        if self.capable(FileSystemCapability::ConditionalRead) {
            assert!(
                matches!(version_support, FixtureSupport::Supported(_)),
                "conditional-read contract: advertised capability requires fixture.resource_version support"
            );
            let bytes = self
                .fixture
                .file_system()
                .read_all(path, conditional, 64)
                .await
                .expect("read contract: advertised conditional read failed");
            assert_eq!(bytes, b"async bytes");
        } else {
            let error = self
                .fixture
                .file_system()
                .open_reader(path, conditional)
                .await
                .expect_err(
                    "read contract: unadvertised conditional read succeeded",
                );
            self.assert_requirement_error(
                &error,
                FsOperation::OpenReader,
                FileSystemCapability::ConditionalRead,
                "conditional-read contract",
            );
        }

        let checksummed = ReadOptions {
            checksum: ChecksumPolicy::Required,
            ..ReadOptions::default()
        };
        if self.capable(FileSystemCapability::ChecksumValidation) {
            let bytes = self
                .fixture
                .file_system()
                .read_all(path, checksummed, 64)
                .await
                .expect("read contract: advertised checksum validation failed");
            assert_eq!(bytes, b"async bytes");
        } else {
            let error = self
                .fixture
                .file_system()
                .open_reader(path, checksummed)
                .await
                .expect_err(
                    "read contract: unadvertised checksum validation succeeded",
                );
            self.assert_requirement_error(
                &error,
                FsOperation::OpenReader,
                FileSystemCapability::ChecksumValidation,
                "checksum-read contract",
            );
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
        self.context.record_created(path.clone());
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
        let outcome = writer
            .commit_async()
            .await
            .expect("write contract: writer commit failed");
        if let Some(bytes_written) = outcome.bytes_written {
            assert_eq!(bytes_written, 13, "writer contract: byte count mismatch");
        }
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
        self.assert_write_options(&path).await;
    }

    /// Checks asynchronous write dispositions, abort, and conditions.
    async fn assert_write_options(&mut self, existing: &qubit_fs::Path) {
        let create_new = WriteOptions {
            disposition: WriteDisposition::CreateNew,
            ..WriteOptions::default()
        };
        let error = match self
            .fixture
            .file_system()
            .open_writer(existing, create_new)
            .await
        {
            Ok(mut writer) => {
                writer
                    .write_fully_async(b"unexpected")
                    .await
                    .expect("writer contract: create-new writer rejected bytes");
                writer.commit_async().await.expect_err(
                    "writer contract: create-new replaced an existing target",
                )
            }
            Err(error) => error,
        };
        assert!(
            matches!(
                error.operation(),
                FsOperation::OpenWriter | FsOperation::CommitWriter
            ),
            "writer contract: create-new failed at an unrelated operation"
        );
        self.assert_error(
            &error,
            FsErrorKind::AlreadyExists,
            error.operation(),
            existing,
        );
        self.assert_bytes(
            existing,
            b"async written",
            "writer contract: failed create-new changed the target",
        )
        .await;

        let mut writer = self
            .fixture
            .file_system()
            .open_writer(existing, WriteOptions::default())
            .await
            .expect("writer contract: replacement writer open failed");
        writer
            .write_fully_async(b"replaced")
            .await
            .expect("writer contract: replacement writer rejected bytes");
        writer
            .commit_async()
            .await
            .expect("writer contract: replacement commit failed");
        self.assert_bytes(
            existing,
            b"replaced",
            "writer contract: replacement bytes mismatch",
        )
        .await;

        let aborted_path = self.path("async-write-aborted");
        self.context.record_created(aborted_path.clone());
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&aborted_path, WriteOptions::default())
            .await
            .expect("writer contract: abort writer open failed");
        writer
            .write_fully_async(b"aborted")
            .await
            .expect("writer contract: abort writer rejected bytes");
        writer
            .abort_async()
            .await
            .expect("writer contract: abort failed");
        assert!(
            !self
                .fixture
                .file_system()
                .exists(&aborted_path)
                .await
                .expect("writer contract: aborted path observation failed"),
            "writer contract: abort published the target"
        );

        let conditional_path = self.path("async-write-conditional");
        let conditional = WriteOptions {
            precondition: WritePrecondition::IfAbsent,
            ..WriteOptions::default()
        };
        if self.capable(FileSystemCapability::ConditionalWrite) {
            self.context.record_created(conditional_path.clone());
            let mut writer = self
                .fixture
                .file_system()
                .open_writer(&conditional_path, conditional.clone())
                .await
                .expect("writer contract: conditional writer open failed");
            writer
                .write_fully_async(b"conditional")
                .await
                .expect("writer contract: conditional writer rejected bytes");
            writer
                .commit_async()
                .await
                .expect("writer contract: conditional commit failed");

            let error = match self
                .fixture
                .file_system()
                .open_writer(&conditional_path, conditional)
                .await
            {
                Ok(mut retry) => {
                    retry
                        .write_fully_async(b"unexpected")
                        .await
                        .expect("writer contract: conditional retry rejected bytes");
                    retry.commit_async().await.expect_err(
                        "writer contract: failed conditional write unexpectedly succeeded",
                    )
                }
                Err(error) => error,
            };
            assert!(
                matches!(
                    error.operation(),
                    FsOperation::OpenWriter | FsOperation::CommitWriter
                ),
                "writer contract: conditional write failed at an unrelated operation"
            );
            self.assert_error(
                &error,
                FsErrorKind::PreconditionFailed,
                error.operation(),
                &conditional_path,
            );
            self.assert_bytes(
                &conditional_path,
                b"conditional",
                "writer contract: failed condition changed target bytes",
            )
            .await;
        } else {
            let error = self
                .fixture
                .file_system()
                .open_writer(&conditional_path, conditional)
                .await
                .expect_err(
                    "writer contract: unadvertised conditional write succeeded",
                );
            self.assert_requirement_error(
                &error,
                FsOperation::OpenWriter,
                FileSystemCapability::ConditionalWrite,
                "conditional-write contract",
            );
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
            let nested = self
                .required_seed(
                    "async-list/prefixed/nested",
                    b"nested",
                    "list",
                )
                .await;
            let nested_second = self
                .required_seed(
                    "async-list/prefixed/second",
                    b"second nested",
                    "list",
                )
                .await;
            let prefix = self
                .fixture
                .list_prefix(&path, "prefixed")
                .expect("list contract: fixture prefix failed");
            let mut stream = self
                .fixture
                .file_system()
                .list(
                    &path,
                    ListOptions {
                        include_metadata: true,
                        prefix: Some(prefix),
                        page_size: Some(1),
                        ..ListOptions::default()
                    },
                )
                .await
                .expect("list contract: prefix listing failed");
            let mut prefixed = Vec::new();
            while let Some(entry) = stream
                .next_entry_async()
                .await
                .expect("list contract: prefix stream error")
            {
                assert!(
                    entry.metadata.is_some(),
                    "list contract: requested entry metadata is missing"
                );
                prefixed.push(entry.path);
            }
            prefixed.sort_by(|left, right| left.as_str().cmp(right.as_str()));
            let mut expected = vec![nested, nested_second];
            let prefix_entry = qubit_fs::Path::parse(
                expected[0]
                    .as_str()
                    .rsplit_once('/')
                    .expect("list contract: nested path must have a parent")
                    .0,
            )
            .expect("list contract: generated prefix path must be valid");
            if prefixed.contains(&prefix_entry) {
                expected.push(prefix_entry);
            }
            expected.sort_by(|left, right| left.as_str().cmp(right.as_str()));
            assert_eq!(
                prefixed, expected,
                "list contract: paged prefix results mismatch"
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
            self.context.record_created(path.clone());
            self.fixture
                .file_system()
                .create_directory(&path, CreateDirectoryOptions::default())
                .await
                .expect(
                    "create-directory contract: advertised creation failed",
                );
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
            let outcome = self
                .fixture
                .file_system()
                .create_directory(
                    &path,
                    CreateDirectoryOptions {
                        exists_ok: true,
                        ..CreateDirectoryOptions::default()
                    },
                )
                .await
                .expect("create-directory contract: existing directory was not accepted");
            assert!(
                outcome.already_existed(),
                "create-directory contract: existing directory outcome was not reported"
            );
            let parent = self.path("async-created-recursive-parent");
            let child = self.path("async-created-recursive-parent/child");
            self.context.record_created(parent);
            self.context.record_created(child.clone());
            let outcome = self
                .fixture
                .file_system()
                .create_directory(
                    &child,
                    CreateDirectoryOptions {
                        recursive: true,
                        ..CreateDirectoryOptions::default()
                    },
                )
                .await
                .expect(
                    "create-directory contract: recursive creation failed",
                );
            if let Some(created_ancestors) = outcome.created_ancestors() {
                assert!(created_ancestors > 0);
            }
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

    /// Checks advertised asynchronous empty-directory and symlink representations.
    pub async fn assert_representations(&mut self) {
        self.context.begin("representations");
        if self.capable(FileSystemCapability::EmptyDirectory) {
            let relative = self.context.relative_name("empty-directory");
            let path = match self
                .fixture
                .seed_empty_directory(&relative)
                .await
                .expect("representation contract: empty-directory setup failed")
            {
                FixtureSupport::Supported(path) => path,
                FixtureSupport::Unsupported => panic!(
                    "representation contract: EmptyDirectory requires fixture.seed_empty_directory support"
                ),
            };
            self.context.record_created(path.clone());
            let metadata = self
                .fixture
                .file_system()
                .stat(&path)
                .await
                .expect("representation contract: empty directory is not statable");
            assert!(
                metadata.is_directory_like(),
                "representation contract: empty directory is not directory-like"
            );
        }
        if self.capable(FileSystemCapability::Symlink) {
            let relative = self.context.relative_name("symlink");
            let path = match self
                .fixture
                .seed_symlink(&relative)
                .await
                .expect("representation contract: symlink setup failed")
            {
                FixtureSupport::Supported(path) => path,
                FixtureSupport::Unsupported => panic!(
                    "representation contract: Symlink requires fixture.seed_symlink support"
                ),
            };
            self.context.record_created(path.clone());
            let metadata = self
                .fixture
                .file_system()
                .stat(&path)
                .await
                .expect("representation contract: symlink is not statable");
            assert_eq!(
                metadata.kind,
                FileKind::Symlink,
                "representation contract: seeded link kind mismatch"
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
            let outcome = self
                .fixture
                .file_system()
                .delete_file(&path, DeleteOptions::default())
                .await
                .expect("delete contract: advertised deletion failed");
            assert!(
                !outcome.already_missing(),
                "delete contract: existing file was reported missing"
            );
            if let Some(deleted_entries) = outcome.deleted_entries() {
                assert!(
                    deleted_entries > 0,
                    "delete contract: deleted count is zero"
                );
            }
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
            self.assert_delete_options().await;
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

    /// Checks asynchronous missing-ok and conditional deletion semantics.
    async fn assert_delete_options(&mut self) {
        let missing = self.path("async-delete-missing-ok");
        let outcome = self
            .fixture
            .file_system()
            .delete_file(
                &missing,
                DeleteOptions {
                    missing_ok: true,
                    ..DeleteOptions::default()
                },
            )
            .await
            .expect("delete contract: missing-ok deletion failed");
        assert!(
            outcome.already_missing(),
            "delete contract: missing-ok outcome did not report absence"
        );

        let path = self.path("async-delete-conditional");
        if self.capable(FileSystemCapability::ConditionalDelete) {
            let path = self
                .required_seed(
                    "async-delete-conditional",
                    b"conditional delete",
                    "conditional-delete",
                )
                .await;
            let version = match self
                .fixture
                .resource_version(&path)
                .await
                .expect("delete contract: version observation failed")
            {
                FixtureSupport::Supported(version) => version,
                FixtureSupport::Unsupported => panic!(
                    "conditional-delete contract: advertised capability requires fixture.resource_version support"
                ),
            };
            self.fixture
                .file_system()
                .delete_file(
                    &path,
                    DeleteOptions {
                        if_match: Some(version),
                        ..DeleteOptions::default()
                    },
                )
                .await
                .expect("delete contract: advertised conditional delete failed");
            assert!(
                !self
                    .fixture
                    .file_system()
                    .exists(&path)
                    .await
                    .expect("delete contract: conditional target observation failed")
            );
        } else {
            let error = self
                .fixture
                .file_system()
                .delete_file(
                    &path,
                    DeleteOptions {
                        if_match: Some(ResourceVersion::new("contract-version")),
                        ..DeleteOptions::default()
                    },
                )
                .await
                .expect_err(
                    "delete contract: unadvertised conditional delete succeeded",
                );
            self.assert_requirement_error(
                &error,
                FsOperation::Delete,
                FileSystemCapability::ConditionalDelete,
                "conditional-delete contract",
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
            assert_error_with_target(
                error.error(),
                FsErrorKind::UnsupportedCapability,
                FsOperation::Copy,
                Some(&source),
                Some(&source),
                Some(self.context.properties().info().provider_id()),
                Some(FileSystemCapability::Copy),
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
        self.context.record_created(target.clone());
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
        assert_eq!(
            outcome.stats().bytes,
            10,
            "async copy contract: copied byte count mismatch"
        );
        assert_eq!(
            outcome.stats().files + outcome.stats().objects,
            1,
            "async copy contract: copied resource count mismatch"
        );
        self.assert_bytes(
            &source,
            b"copy bytes",
            "async copy contract: source was modified",
        )
        .await;
        self.assert_copy_conflicts(&source).await;
        self.assert_bytes(
            &target,
            b"copy bytes",
            "async copy contract: target bytes mismatch",
        )
        .await;
        if self.capable(FileSystemCapability::CreateDirectory) {
            let directory_source = self.path("async-copy-directory-source");
            self.fixture
                .file_system()
                .create_directory(
                    &directory_source,
                    CreateDirectoryOptions::default(),
                )
                .await
                .expect("async copy contract: directory source creation failed");
            self.context.record_created(directory_source.clone());
            let directory_child = self
                .required_seed(
                    "async-copy-directory-source/child",
                    b"directory copy",
                    "copy",
                )
                .await;
            let directory_target = self.path("async-copy-directory-target");
            self.context.record_created(directory_target.clone());
            let target_child = self.path("async-copy-directory-target/child");
            self.context.record_created(target_child.clone());
            let mut operation = self
                .fixture
                .file_system()
                .begin_copy(
                    directory_source,
                    directory_target.clone(),
                    CopyOptions::tree(),
                )
                .expect("async copy contract: directory copy preflight failed");
            operation
                .execute()
                .await
                .expect("async copy contract: directory copy failed");
            self.assert_bytes(
                &target_child,
                b"directory copy",
                "async copy contract: directory child bytes mismatch",
            )
            .await;
            self.context.record_created(directory_child);
        }
        if self.capable(FileSystemCapability::ServerSideCopy) {
            let case = match self
                .fixture
                .copy_fast_path_case(CopyMethod::ServerSide)
                .await
                .expect("async copy contract: fast-path setup failed")
            {
                FixtureSupport::Supported(case) => case,
                FixtureSupport::Unsupported => panic!(
                    "async copy contract: advertised server-side capability lacks an applicable fixture case"
                ),
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
                .expect("async copy contract: server-side preflight failed");
            let outcome = operation
                .execute()
                .await
                .expect("async copy contract: server-side copy failed");
            assert_eq!(
                outcome.method(),
                CopyMethod::ServerSide,
                "async copy contract: reported server-side method mismatch"
            );
            assert!(!outcome.used_fallback());
        } else {
            let source = self.path("async-copy-server-side-unavailable-source");
            let target = self.path("async-copy-server-side-unavailable-target");
            let error = match self.fixture.file_system().begin_copy(
                source,
                target,
                CopyOptions {
                    server_side: ServerSidePreference::Require,
                    ..CopyOptions::default()
                },
            ) {
                Err(error) => error,
                Ok(_) => panic!(
                    "async copy contract: unadvertised server-side copy succeeded"
                ),
            };
            self.assert_requirement_error(
                error.error(),
                FsOperation::Copy,
                FileSystemCapability::ServerSideCopy,
                "server-side-copy contract",
            );
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

    /// Checks asynchronous destination conflict policies and statistics.
    async fn assert_copy_conflicts(&mut self, source: &qubit_fs::Path) {
        let target = self
            .required_seed(
                "async-copy-conflict-target",
                b"existing",
                "copy-conflict",
            )
            .await;
        let mut operation = self
            .fixture
            .file_system()
            .begin_copy(source.clone(), target.clone(), CopyOptions::file())
            .expect("copy contract: conflict preflight failed");
        let failure = operation
            .execute()
            .await
            .expect_err("copy contract: default conflict replaced target");
        assert_error_with_target(
            failure.error(),
            FsErrorKind::AlreadyExists,
            FsOperation::Copy,
            Some(source),
            Some(&target),
            Some(self.context.properties().info().provider_id()),
            None,
        );
        self.assert_bytes(
            &target,
            b"existing",
            "copy contract: failed conflict changed target",
        )
        .await;

        let mut operation = self
            .fixture
            .file_system()
            .begin_copy(
                source.clone(),
                target.clone(),
                CopyOptions {
                    mode: qubit_fs::CopyMode::File,
                    conflict: CopyConflictPolicy::Skip,
                    ..CopyOptions::default()
                },
            )
            .expect("copy contract: skip preflight failed");
        let skipped = operation
            .execute()
            .await
            .expect("copy contract: skip conflict failed");
        assert_eq!(skipped.stats().skipped, 1);
        self.assert_bytes(
            &target,
            b"existing",
            "copy contract: skipped copy changed target",
        )
        .await;

        let mut operation = self
            .fixture
            .file_system()
            .begin_copy(
                source.clone(),
                target.clone(),
                CopyOptions {
                    mode: qubit_fs::CopyMode::File,
                    conflict: CopyConflictPolicy::Overwrite,
                    ..CopyOptions::default()
                },
            )
            .expect("copy contract: overwrite preflight failed");
        let overwritten = operation
            .execute()
            .await
            .expect("copy contract: overwrite conflict failed");
        assert_eq!(overwritten.stats().overwritten, 1);
        self.assert_bytes(
            &target,
            b"copy bytes",
            "copy contract: overwrite bytes mismatch",
        )
        .await;
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
            self.context.record_created(target.clone());
            let outcome = self
                .fixture
                .file_system()
                .rename(&source, &target, RenameOptions::default())
                .await
                .expect("rename contract: advertised rename failed");
            assert_eq!(
                outcome.source(),
                &source,
                "rename contract: outcome source mismatch"
            );
            assert_eq!(
                outcome.target(),
                &target,
                "rename contract: outcome target mismatch"
            );
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
            self.assert_rename_conflicts().await;
        } else {
            let failure = self
                .fixture
                .file_system()
                .rename(&source, &target, RenameOptions::default())
                .await
                .expect_err("rename contract: unadvertised rename succeeded");
            assert_error_with_target(
                failure.error(),
                FsErrorKind::UnsupportedCapability,
                FsOperation::Rename,
                Some(&source),
                Some(&target),
                Some(self.context.properties().info().provider_id()),
                Some(FileSystemCapability::Rename),
            );
        }
    }

    /// Checks asynchronous rename conflicts and explicit overwrite.
    async fn assert_rename_conflicts(&mut self) {
        let source = self
            .required_seed(
                "async-rename-conflict-source",
                b"rename source",
                "rename-conflict",
            )
            .await;
        let target = self
            .required_seed(
                "async-rename-conflict-target",
                b"rename target",
                "rename-conflict",
            )
            .await;
        let failure = self
            .fixture
            .file_system()
            .rename(&source, &target, RenameOptions::default())
            .await
            .expect_err("rename contract: default conflict replaced target");
        assert_eq!(failure.state(), RenameFailureState::Unchanged);
        assert_error_with_target(
            failure.error(),
            FsErrorKind::AlreadyExists,
            FsOperation::Rename,
            Some(&source),
            Some(&target),
            Some(self.context.properties().info().provider_id()),
            None,
        );
        self.assert_bytes(
            &source,
            b"rename source",
            "rename contract: conflict removed source",
        )
        .await;
        self.assert_bytes(
            &target,
            b"rename target",
            "rename contract: conflict changed target",
        )
        .await;

        self.fixture
            .file_system()
            .rename(
                &source,
                &target,
                RenameOptions {
                    overwrite: true,
                    ..RenameOptions::default()
                },
            )
            .await
            .expect("rename contract: overwrite failed");
        assert!(
            !self
                .fixture
                .file_system()
                .exists(&source)
                .await
                .expect("rename contract: overwrite source observation failed")
        );
        self.assert_bytes(
            &target,
            b"rename source",
            "rename contract: overwrite target mismatch",
        )
        .await;
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
        if self.capable(FileSystemCapability::CreateDirectory) {
            self.fixture
                .file_system()
                .create_directory(&root, CreateDirectoryOptions::default())
                .await
                .expect("recursive-delete contract: root creation failed");
        }
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
        self.context.record_created(target.clone());
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
        self.context.record_created(path.clone());
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
        self.context.record_created(target.clone());
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
            self.assert_temp_file_options().await;
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
            let mut temporary = self
                .fixture
                .file_system()
                .create_temp_file(TempFileOptions::default())
                .await
                .expect("temp-file contract: persist setup failed");
            let target = self.path("async-temp-persisted-file");
            self.context.record_created(target.clone());
            self.assert_temp_file_persist(&mut temporary, &target).await;
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
            self.assert_temp_directory_options().await;
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
            let mut temporary = self
                .fixture
                .file_system()
                .create_temp_directory(TempDirectoryOptions::default())
                .await
                .expect("temp-directory contract: persist setup failed");
            let target = self.path("async-temp-persisted-directory");
            self.context.record_created(target.clone());
            self.assert_temp_directory_persist(&mut temporary, &target)
                .await;
            if self.capable(FileSystemCapability::CreateDirectory) {
                self.assert_temp_directory_overwrite().await;
            }
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

    /// Verifies asynchronous temporary-file persistence publication.
    async fn assert_temp_file_persist(
        &self,
        temporary: &mut qubit_fs::AsyncTempFile,
        target: &qubit_fs::Path,
    ) {
        let outcome = temporary
            .persist(target, self.temp_persist_options())
            .await
            .expect("temp-file contract: persist failed");
        self.assert_temp_persist_outcome(
            &outcome,
            target,
            "temp-file contract",
        )
        .await;
        if !self.capable(FileSystemCapability::AtomicTempPersist) {
            let mut retry = self
                .fixture
                .file_system()
                .create_temp_file(TempFileOptions::default())
                .await
                .expect("temp-file contract: atomic preflight setup failed");
            let source = retry.path().clone();
            let failure = retry
                .persist(
                    &self.path("async-temp-required-atomic-file"),
                    PersistOptions::default(),
                )
                .await
                .expect_err("temp-file contract: unadvertised required atomic persist succeeded");
            assert_eq!(
                failure.state(),
                PersistFailureState::NotPublished,
                "temp-file contract: failed preflight changed publication responsibility"
            );
            self.assert_requirement_error(
                failure.error(),
                FsOperation::PersistTemp,
                FileSystemCapability::AtomicTempPersist,
                "temp-file contract",
            );
            assert!(
                self.fixture
                    .file_system()
                    .exists(&source)
                    .await
                    .expect("temp-file contract: source exists failed"),
                "temp-file contract: required atomic preflight removed source"
            );
            retry
                .cleanup()
                .await
                .expect("temp-file contract: retained source cleanup failed");
        }
    }

    /// Verifies asynchronous temporary-directory persistence publication.
    async fn assert_temp_directory_persist(
        &self,
        temporary: &mut qubit_fs::AsyncTempDirectory,
        target: &qubit_fs::Path,
    ) {
        let outcome = temporary
            .persist(target, self.temp_persist_options())
            .await
            .expect("temp-directory contract: persist failed");
        self.assert_temp_persist_outcome(
            &outcome,
            target,
            "temp-directory contract",
        )
        .await;
        if !self.capable(FileSystemCapability::AtomicTempPersist) {
            let mut retry = self
                .fixture
                .file_system()
                .create_temp_directory(TempDirectoryOptions::default())
                .await
                .expect("temp-directory contract: atomic preflight setup failed");
            let source = retry.path().clone();
            let failure = retry
                .persist(
                    &self.path("async-temp-required-atomic-directory"),
                    PersistOptions::default(),
                )
                .await
                .expect_err("temp-directory contract: unadvertised required atomic persist succeeded");
            assert_eq!(
                failure.state(),
                PersistFailureState::NotPublished,
                "temp-directory contract: failed preflight changed publication responsibility"
            );
            self.assert_requirement_error(
                failure.error(),
                FsOperation::PersistTemp,
                FileSystemCapability::AtomicTempPersist,
                "temp-directory contract",
            );
            assert!(
                self.fixture
                    .file_system()
                    .exists(&source)
                    .await
                    .expect("temp-directory contract: source exists failed"),
                "temp-directory contract: required atomic preflight removed source"
            );
            retry
                .cleanup()
                .await
                .expect("temp-directory contract: retained source cleanup failed");
        }
    }

    /// Verifies asynchronous overwrite publication for an empty directory.
    async fn assert_temp_directory_overwrite(&mut self) {
        let target = self.path("async-temp-overwritten-directory");
        self.context.record_created(target.clone());
        self.fixture
            .file_system()
            .create_directory(&target, CreateDirectoryOptions::default())
            .await
            .expect("temp-directory overwrite contract: destination setup failed");
        let mut temporary = self
            .fixture
            .file_system()
            .create_temp_directory(TempDirectoryOptions::default())
            .await
            .expect("temp-directory overwrite contract: temporary creation failed");
        let outcome = temporary
            .persist(
                &target,
                PersistOptions {
                    overwrite: true,
                    ..self.temp_persist_options()
                },
            )
            .await
            .expect("temp-directory overwrite contract: persist failed");
        assert_eq!(outcome.target, target);
        assert!(
            self.fixture
                .file_system()
                .exists(&target)
                .await
                .expect("temp-directory overwrite contract: target observation failed")
        );
    }

    /// Checks asynchronous temporary-file parent and affix options.
    ///
    /// # Panics
    ///
    /// Panics when a temporary file ignores the requested parent, prefix, or
    /// suffix, or when cleanup fails.
    async fn assert_temp_file_options(&mut self) {
        let parent = self
            .prepare_temp_options_parent("async-temp-file-options-parent")
            .await;
        let mut temporary = self
            .fixture
            .file_system()
            .create_temp_file(TempFileOptions {
                parent: parent.clone(),
                prefix: "async-file-".to_owned(),
                suffix: ".tmp".to_owned(),
            })
            .await
            .expect("temp-file contract: option-aware creation failed");
        self.assert_temp_path(
            temporary.path(),
            parent.as_ref(),
            "async-file-",
            ".tmp",
            "temp-file contract",
        );
        temporary
            .cleanup()
            .await
            .expect("temp-file contract: option-aware cleanup failed");
    }

    /// Checks asynchronous temporary-directory parent and affix options.
    ///
    /// # Panics
    ///
    /// Panics when a temporary directory ignores the requested parent, prefix,
    /// or suffix, or when cleanup fails.
    async fn assert_temp_directory_options(&mut self) {
        let parent = self
            .prepare_temp_options_parent("async-temp-directory-options-parent")
            .await;
        let mut temporary = self
            .fixture
            .file_system()
            .create_temp_directory(TempDirectoryOptions {
                parent: parent.clone(),
                prefix: "async-directory-".to_owned(),
                suffix: ".tmpdir".to_owned(),
            })
            .await
            .expect("temp-directory contract: option-aware creation failed");
        self.assert_temp_path(
            temporary.path(),
            parent.as_ref(),
            "async-directory-",
            ".tmpdir",
            "temp-directory contract",
        );
        temporary
            .cleanup()
            .await
            .expect("temp-directory contract: option-aware cleanup failed");
    }

    /// Prepares the parent directory used by temporary option assertions.
    ///
    /// # Returns
    ///
    /// The created parent path when directory creation is advertised, or `None`
    /// otherwise.
    ///
    /// # Panics
    ///
    /// Panics when an advertised directory creation operation fails.
    async fn prepare_temp_options_parent(
        &mut self,
        relative: &str,
    ) -> Option<qubit_fs::Path> {
        if !self.capable(FileSystemCapability::CreateDirectory) {
            return None;
        }
        let parent = self.path(relative);
        self.fixture
            .file_system()
            .create_directory(
                &parent,
                CreateDirectoryOptions::default(),
            )
            .await
            .expect("temporary resource contract: option parent creation failed");
        self.context.record_created(parent.clone());
        Some(parent)
    }

    /// Checks that a temporary resource honors its requested location and name.
    ///
    /// # Parameters
    ///
    /// * `path` - Provider-created temporary resource path.
    /// * `parent` - Requested parent path, when one was requested.
    /// * `prefix` - Requested filename prefix.
    /// * `suffix` - Requested filename suffix.
    /// * `contract` - Contract label used in diagnostics.
    ///
    /// # Panics
    ///
    /// Panics when the path is not an immediate child of an explicitly
    /// requested `parent`, or does not honor the requested affixes.
    fn assert_temp_path(
        &self,
        path: &qubit_fs::Path,
        parent: Option<&qubit_fs::Path>,
        prefix: &str,
        suffix: &str,
        contract: &str,
    ) {
        let name = path
            .as_str()
            .rsplit('/')
            .next()
            .expect("temporary resource path must have a final component");
        if let Some(parent) = parent {
            let parent_prefix =
                format!("{}/", parent.as_str().trim_end_matches('/'));
            assert!(
                path.as_str().strip_prefix(&parent_prefix).is_some_and(
                    |relative| !relative.contains('/'),
                ),
                "{contract}: temporary resource was not an immediate child of the requested parent"
            );
        }
        assert!(
            name.starts_with(prefix),
            "{contract}: temporary resource prefix mismatch"
        );
        assert!(
            name.ends_with(suffix),
            "{contract}: temporary resource suffix mismatch"
        );
    }

    /// Builds persistence options matching the advertised atomic guarantee.
    #[inline]
    fn temp_persist_options(&self) -> PersistOptions {
        PersistOptions {
            atomicity: if self.capable(FileSystemCapability::AtomicTempPersist)
            {
                AtomicityRequirement::Required
            } else {
                AtomicityRequirement::Preferred
            },
            ..PersistOptions::default()
        }
    }

    /// Checks persistence reporting and destination publication.
    async fn assert_temp_persist_outcome(
        &self,
        outcome: &qubit_fs::PersistOutcome,
        target: &qubit_fs::Path,
        label: &str,
    ) {
        assert_eq!(outcome.target, *target, "{label}: persist target mismatch");
        if self.capable(FileSystemCapability::AtomicTempPersist) {
            assert_eq!(
                outcome.atomicity,
                AchievedAtomicity::Atomic,
                "{label}: required operation reported non-atomic publication"
            );
        }
        assert!(
            self.fixture
                .file_system()
                .exists(target)
                .await
                .expect("temporary resource contract: target exists failed"),
            "{label}: persist did not publish target"
        );
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
        let provider = Some(self.context.properties().info().provider_id());
        if kind == FsErrorKind::UnsupportedCapability {
            assert_unsupported_error(
                error,
                kind,
                operation,
                Some(path),
                provider,
                error.required_capability(),
            );
        } else {
            assert_error_with_target(
                error,
                kind,
                operation,
                Some(path),
                None,
                provider,
                error.required_capability(),
            );
        }
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
        let _ = contract;
        assert_unsupported_error(
            error,
            FsErrorKind::RequirementNotMet,
            operation,
            error.path(),
            Some(self.context.properties().info().provider_id()),
            Some(capability),
        );
    }
}
