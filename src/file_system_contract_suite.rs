// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stateful synchronous filesystem provider contract suite.

use qubit_fs::{
    AchievedAtomicity, AtomicityRequirement, CopyMethod, CopyOptions, CreateDirectoryOptions,
    DeleteOptions, DurabilityRequirement, FileKind, FileSystemCapability, FsErrorKind, FsOperation,
    ListOptions, PersistFailureState, PersistOptions, RenameOptions, TempDirectory,
    TempDirectoryOptions, TempFile, TempFileOptions, WriteDisposition, WriteOptions,
};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use crate::contract_context::ContractContext;
use crate::{FileSystemFixture, FixtureSupport};

/// Runs synchronous provider contracts against one isolated fixture.
pub struct FileSystemContractSuite<'a> {
    fixture: &'a dyn FileSystemFixture,
    context: ContractContext,
}

impl<'a> FileSystemContractSuite<'a> {
    /// Creates a stateful suite borrowing one isolated provider fixture.
    #[must_use]
    pub fn new(fixture: &'a dyn FileSystemFixture) -> Self {
        Self {
            fixture,
            context: ContractContext::new(fixture.file_system().properties()),
        }
    }

    /// Runs all synchronous contracts in their dependency-safe fixed order.
    pub fn assert_all(mut self) {
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.assert_properties();
            self.assert_stat();
            self.assert_read();
            self.assert_write();
            self.assert_list();
            self.assert_create_directory();
            self.assert_delete();
            self.assert_copy();
            self.assert_rename();
            self.assert_append();
            self.assert_recursive_delete();
            self.assert_atomic_rename();
            self.assert_atomic_replace();
            self.assert_durable_copy();
            self.assert_temp_resources();
            self.assert_error_context();
        }));
        self.finish();
        if let Err(payload) = result {
            resume_unwind(payload);
        }
    }

    /// Cleans resources created by individually executed contract phases.
    pub fn finish(&mut self) {
        self.context.cleanup(self.fixture.file_system());
    }

    /// Checks immutable facade properties and fixture path compatibility.
    pub fn assert_properties(&mut self) {
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

    /// Checks metadata behavior.
    pub fn assert_stat(&mut self) {
        self.context.begin("stat");
        let file_system = self.fixture.file_system();
        let missing = self.path("stat-missing");
        let error = file_system
            .stat(&missing)
            .expect_err("stat contract: missing path succeeded");
        self.assert_error(
            &error,
            FsErrorKind::NotFound,
            FsOperation::Stat,
            &missing,
            None,
        );

        if let FixtureSupport::Supported(path) = self.seed("stat-file", b"stateful stat") {
            self.context.record_created(path.clone());
            let metadata = file_system
                .stat(&path)
                .expect("stat contract: seeded file is not statable");
            assert_eq!(
                metadata.kind,
                FileKind::File,
                "stat contract: file kind mismatch"
            );
            assert_eq!(
                metadata.len,
                Some(13),
                "stat contract: file length mismatch"
            );
        }
    }

    /// Checks reader behavior.
    pub fn assert_read(&mut self) {
        self.context.begin("read");
        let file_system = self.fixture.file_system();
        if !self.capable(FileSystemCapability::Read) {
            let path = self.path("read-unavailable");
            let error = file_system
                .open_reader(&path, Default::default())
                .expect_err("read contract: unadvertised reader open succeeded");
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::OpenReader,
                &path,
                None,
            );
            assert_eq!(
                error.required_capability(),
                Some(FileSystemCapability::Read),
                "read contract: missing required-capability context"
            );
            return;
        }
        let path = self.required_seed("read-file", b"read contract bytes", "read");
        self.context.record_created(path.clone());
        let bytes = file_system
            .read_all(&path, Default::default(), 64)
            .expect("read contract: facade could not read seeded bytes");
        assert_eq!(
            bytes, b"read contract bytes",
            "read contract: bytes mismatch"
        );
        let error = file_system
            .read_all(&path, Default::default(), 4)
            .expect_err("read contract: caller byte limit was ignored");
        self.assert_error(
            &error,
            FsErrorKind::ResourceLimitExceeded,
            FsOperation::Read,
            &path,
            None,
        );
    }

    /// Checks writer behavior.
    pub fn assert_write(&mut self) {
        self.context.begin("write");
        if !self.capable(FileSystemCapability::Write) {
            let path = self.path("write-unavailable");
            let error = self
                .fixture
                .file_system()
                .open_writer(&path, Default::default())
                .expect_err("writer contract: unadvertised writer open succeeded");
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::OpenWriter,
                &path,
                None,
            );
            assert_eq!(
                error.required_capability(),
                Some(FileSystemCapability::Write),
                "writer contract: missing required-capability context"
            );
            return;
        }
        let path = self.path("write-file");
        self.fixture
            .file_system()
            .write_all(&path, b"written", WriteOptions::default())
            .expect("writer contract: write failed");
        match self
            .fixture
            .read_file(&path)
            .expect("writer contract: fixture observation failed")
        {
            FixtureSupport::Supported(bytes) => {
                assert_eq!(bytes, b"written", "I/O contract: write was not published")
            }
            FixtureSupport::Unsupported => {
                panic!("writer contract: Write capability requires fixture.read_file support")
            }
        }
        self.context.record_created(path);
    }

    /// Checks directory listing behavior.
    pub fn assert_list(&mut self) {
        self.context.begin("list");
        let root = self.path("list-root");
        if !self.capable(FileSystemCapability::List) {
            let error = self
                .fixture
                .file_system()
                .list(&root, Default::default())
                .expect_err("list contract: unadvertised listing succeeded");
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::List,
                &root,
                None,
            );
            assert_eq!(
                error.required_capability(),
                Some(FileSystemCapability::List),
                "list contract: missing required-capability context"
            );
            return;
        }
        let first = self.required_seed("list-root/first", b"first", "list");
        self.context.record_created(first.clone());
        let second = self.required_seed("list-root/second", b"second", "list");
        self.context.record_created(second.clone());
        let mut stream = self
            .fixture
            .file_system()
            .list(&root, Default::default())
            .expect("list contract: cannot open namespace");
        let mut actual = Vec::new();
        while let Some(entry) = stream.next_entry().expect("list contract: stream error") {
            actual.push(entry.path);
        }
        actual.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        let mut expected = vec![first, second];
        expected.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        assert_eq!(actual, expected, "list contract: direct children mismatch");

        let nested = self.required_seed("list-root/prefixed/nested", b"nested", "list");
        self.context.record_created(nested.clone());
        let prefix = self
            .fixture
            .list_prefix(&root, "prefixed/nested")
            .expect("list contract: fixture prefix failed");
        let mut stream = self
            .fixture
            .file_system()
            .list(
                &root,
                ListOptions {
                    include_metadata: true,
                    prefix: Some(prefix),
                    page_size: Some(1),
                    ..ListOptions::default()
                },
            )
            .expect("list contract: prefix listing failed");
        let entry = stream
            .next_entry()
            .expect("list contract: prefix stream error")
            .expect("list contract: nested prefix result missing");
        assert_eq!(nested, entry.path, "list contract: prefix mismatch");
        assert_eq!(
            entry.metadata.and_then(|metadata| metadata.len),
            Some(6),
            "list contract: requested entry metadata is missing or incorrect"
        );
        assert!(
            stream
                .next_entry()
                .expect("list contract: prefix stream error")
                .is_none(),
            "list contract: prefix returned extra entries"
        );
    }

    /// Checks directory creation behavior.
    pub fn assert_create_directory(&mut self) {
        self.context.begin("create_directory");
        if !self.capable(FileSystemCapability::CreateDirectory) {
            let path = self.path("create-directory-unavailable");
            let error = self
                .fixture
                .file_system()
                .create_directory(&path, CreateDirectoryOptions::default())
                .expect_err("create-directory contract: unadvertised creation succeeded");
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::CreateDir,
                &path,
                None,
            );
            assert_eq!(
                error.required_capability(),
                Some(FileSystemCapability::CreateDirectory),
                "create-directory contract: missing required-capability context"
            );
            return;
        }
        let path = self.path("created-directory");
        self.fixture
            .file_system()
            .create_directory(&path, CreateDirectoryOptions::default())
            .expect("namespace contract: directory creation failed");
        let metadata = self
            .fixture
            .file_system()
            .stat(&path)
            .expect("namespace contract: created directory is missing");
        assert!(
            metadata.is_directory_like(),
            "namespace contract: created path is not a directory"
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
            .expect("namespace contract: existing directory was not accepted");
        assert!(
            outcome.already_existed(),
            "namespace contract: existing directory outcome was not reported"
        );
        self.context.record_created(path);
    }

    /// Checks deletion behavior.
    pub fn assert_delete(&mut self) {
        self.context.begin("delete");
        if !self.capable(FileSystemCapability::Delete) {
            let path = self.path("delete-unavailable");
            let error = self
                .fixture
                .file_system()
                .delete_file(&path, DeleteOptions::default())
                .expect_err("delete contract: unadvertised deletion succeeded");
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::Delete,
                &path,
                None,
            );
            assert_eq!(
                error.required_capability(),
                Some(FileSystemCapability::Delete),
                "delete contract: missing required-capability context"
            );
            return;
        }
        let path = self.required_seed("delete-file", b"delete", "delete");
        self.context.record_created(path.clone());
        self.fixture
            .file_system()
            .delete_file(&path, DeleteOptions::default())
            .expect("delete contract: deletion failed");
        assert!(
            !self
                .fixture
                .file_system()
                .exists(&path)
                .expect("delete contract: exists failed"),
            "delete contract: deleted file remained"
        );
    }

    /// Checks native and fallback copy behavior.
    pub fn assert_copy(&mut self) {
        self.context.begin("copy");
        if !self.capable(FileSystemCapability::Copy) {
            let source = self.path("copy-unavailable");
            let error = self
                .fixture
                .file_system()
                .copy(&source, &source, CopyOptions::default())
                .expect_err("copy contract: unadvertised copy succeeded");
            self.assert_error(
                error.error(),
                FsErrorKind::UnsupportedCapability,
                FsOperation::Copy,
                &source,
                Some(&source),
            );
            assert_eq!(
                error.error().required_capability(),
                Some(FileSystemCapability::Copy),
                "copy contract: missing required-capability context"
            );
            return;
        }
        let source = self.required_seed("copy-source", b"copy bytes", "copy");
        self.context.record_created(source.clone());
        let target = self.path("copy-target");
        let outcome = self
            .fixture
            .file_system()
            .copy(&source, &target, CopyOptions::default())
            .expect("copy contract: fallback copy failed");
        match outcome.method() {
            CopyMethod::Streamed => assert!(
                outcome.used_fallback(),
                "copy contract: streamed copy was not reported as fallback"
            ),
            CopyMethod::Native | CopyMethod::Clone | CopyMethod::ServerSide | CopyMethod::Mixed => {
                assert!(
                    !outcome.used_fallback(),
                    "copy contract: completed fast path was reported as fallback"
                )
            }
        }
        self.assert_bytes(&source, b"copy bytes", "copy contract: source was modified");
        self.assert_bytes(
            &target,
            b"copy bytes",
            "copy contract: target bytes mismatch",
        );
        self.context.record_created(target);
        if self.capable(FileSystemCapability::CreateDirectory) {
            let directory_source = self.path("copy-directory-source");
            self.fixture
                .file_system()
                .create_directory(&directory_source, CreateDirectoryOptions::default())
                .expect("copy contract: directory source creation failed");
            self.context.record_created(directory_source.clone());
            let directory_child =
                self.required_seed("copy-directory-source/child", b"directory copy", "copy");
            let directory_target = self.path("copy-directory-target");
            self.fixture
                .file_system()
                .copy(&directory_source, &directory_target, CopyOptions::tree())
                .expect("copy contract: directory copy failed");
            self.context.record_created(directory_target.clone());
            let target_child = self.path("copy-directory-target/child");
            self.assert_bytes(
                &target_child,
                b"directory copy",
                "copy contract: directory child bytes mismatch",
            );
            self.context.record_created(target_child);
            self.context.record_created(directory_child);
        }
        if self.capable(FileSystemCapability::ServerSideCopy) {
            match self
                .fixture
                .copy_fast_path_case(CopyMethod::ServerSide)
                .expect("copy contract: fixture fast-path setup failed")
            {
                FixtureSupport::Supported(case) => {
                    let outcome = self
                        .fixture
                        .file_system()
                        .copy(case.source(), case.target(), case.options().clone())
                        .expect("copy contract: native case failed");
                    assert_eq!(
                        outcome.method(),
                        CopyMethod::ServerSide,
                        "copy contract: reported method mismatch"
                    );
                    assert!(
                        !outcome.used_fallback(),
                        "copy contract: native case unexpectedly fell back"
                    );
                    self.context.record_created(case.source().clone());
                    self.context.record_created(case.target().clone());
                }
                FixtureSupport::Unsupported => panic!(
                    "copy contract: advertised native capability lacks an applicable fixture case"
                ),
            }
        }
    }

    /// Checks rename behavior.
    pub fn assert_rename(&mut self) {
        self.context.begin("rename");
        if !self.capable(FileSystemCapability::Rename) {
            let source = self.path("rename-unavailable-source");
            let target = self.path("rename-unavailable-target");
            let failure = self
                .fixture
                .file_system()
                .rename(&source, &target, RenameOptions::default())
                .expect_err("rename contract: unadvertised rename succeeded");
            self.assert_error(
                failure.error(),
                FsErrorKind::UnsupportedCapability,
                FsOperation::Rename,
                &source,
                Some(&target),
            );
            assert_eq!(
                failure.error().required_capability(),
                Some(FileSystemCapability::Rename),
                "rename contract: missing required-capability context"
            );
            return;
        }
        let source = self.required_seed("rename-source", b"rename", "rename");
        self.context.record_created(source.clone());
        let target = self.path("rename-target");
        let outcome = self
            .fixture
            .file_system()
            .rename(&source, &target, RenameOptions::default())
            .expect("rename contract: rename failed");
        assert_eq!(
            outcome.source(),
            &source,
            "rename contract: source context mismatch"
        );
        assert_eq!(
            outcome.target(),
            &target,
            "rename contract: target context mismatch"
        );
        assert!(
            !self
                .fixture
                .file_system()
                .exists(&source)
                .expect("rename contract: source exists failed"),
            "rename contract: source remained after success"
        );
        assert!(
            self.fixture
                .file_system()
                .exists(&target)
                .expect("rename contract: target exists failed"),
            "rename contract: target missing after success"
        );
        self.context.record_created(target);
    }

    /// Checks append writes when the provider advertises that guarantee.
    pub fn assert_append(&mut self) {
        self.context.begin("append");
        let path = self.path("append-target");
        let options = WriteOptions {
            disposition: WriteDisposition::Append,
            ..WriteOptions::default()
        };
        if !self.capable(FileSystemCapability::Append) {
            let error = self
                .fixture
                .file_system()
                .open_writer(&path, options)
                .expect_err("append contract: unadvertised append preflight succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::OpenWriter,
                FileSystemCapability::Append,
                "append contract",
            );
            return;
        }
        let path = self.required_seed("append-target", b"before", "append");
        self.fixture
            .file_system()
            .write_all(&path, b"-after", options)
            .expect("append contract: append failed");
        self.assert_bytes(
            &path,
            b"before-after",
            "append contract: existing bytes were not retained",
        );
        self.context.record_created(path);
    }

    /// Checks recursive directory removal when the provider advertises it.
    pub fn assert_recursive_delete(&mut self) {
        self.context.begin("recursive_delete");
        let root = self.path("recursive-delete-root");
        let options = DeleteOptions {
            recursive: true,
            ..DeleteOptions::default()
        };
        if !self.capable(FileSystemCapability::RecursiveDelete) {
            let error = self
                .fixture
                .file_system()
                .delete_directory(&root, options)
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
            .expect("recursive-delete contract: root creation failed");
        let child = self.required_seed("recursive-delete-root/child", b"child", "recursive-delete");
        self.context.record_created(child.clone());
        self.context.record_created(root.clone());
        self.fixture
            .file_system()
            .delete_directory(&root, options)
            .expect("recursive-delete contract: recursive removal failed");
        assert!(
            !self
                .fixture
                .file_system()
                .exists(&root)
                .expect("recursive-delete contract: root existence check failed"),
            "recursive-delete contract: root remained after removal"
        );
        assert!(
            !self
                .fixture
                .file_system()
                .exists(&child)
                .expect("recursive-delete contract: child existence check failed"),
            "recursive-delete contract: child remained after removal"
        );
    }

    /// Checks required-atomic rename publication when advertised.
    pub fn assert_atomic_rename(&mut self) {
        self.context.begin("atomic_rename");
        let source = self.path("atomic-rename-source");
        let target = self.path("atomic-rename-target");
        let options = RenameOptions {
            atomicity: AtomicityRequirement::Required,
            ..RenameOptions::default()
        };
        if !self.capable(FileSystemCapability::AtomicRename) {
            let failure = self
                .fixture
                .file_system()
                .rename(&source, &target, options)
                .expect_err("atomic-rename contract: unadvertised preflight succeeded");
            self.assert_requirement_error(
                failure.error(),
                FsOperation::Rename,
                FileSystemCapability::AtomicRename,
                "atomic-rename contract",
            );
            return;
        }
        let source = self.required_seed("atomic-rename-source", b"atomic rename", "atomic-rename");
        let target = self.path("atomic-rename-target");
        let outcome = self
            .fixture
            .file_system()
            .rename(&source, &target, options)
            .expect("atomic-rename contract: required-atomic rename failed");
        assert_eq!(
            outcome.atomicity(),
            AchievedAtomicity::Atomic,
            "atomic-rename contract: required operation reported non-atomic publication"
        );
        self.context.record_created(target);
    }

    /// Checks required-atomic replacement when the provider advertises it.
    pub fn assert_atomic_replace(&mut self) {
        self.context.begin("atomic_replace");
        let path = self.path("atomic-replace-target");
        let options = WriteOptions {
            atomicity: AtomicityRequirement::Required,
            ..WriteOptions::default()
        };
        if !self.capable(FileSystemCapability::AtomicReplace) {
            let error = self
                .fixture
                .file_system()
                .open_writer(&path, options)
                .expect_err("atomic-replace contract: unadvertised preflight succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::OpenWriter,
                FileSystemCapability::AtomicReplace,
                "atomic-replace contract",
            );
            return;
        }
        let outcome = self
            .fixture
            .file_system()
            .write_all(&path, b"atomic replacement", options)
            .expect("atomic-replace contract: required-atomic write failed");
        assert_eq!(
            outcome.atomicity,
            AchievedAtomicity::Atomic,
            "atomic-replace contract: required operation reported non-atomic publication"
        );
        self.context.record_created(path);
    }

    /// Checks required-durable copy publication when the provider advertises
    /// it.
    pub fn assert_durable_copy(&mut self) {
        self.context.begin("durable_copy");
        let source = self.path("durable-copy-source");
        let target = self.path("durable-copy-target");
        let options = CopyOptions {
            durability: DurabilityRequirement::Required,
            ..CopyOptions::default()
        };
        if !self.capable(FileSystemCapability::DurableCopy) {
            let failure = self
                .fixture
                .file_system()
                .copy(&source, &target, options)
                .expect_err("durable-copy contract: unadvertised preflight succeeded");
            self.assert_requirement_error(
                failure.error(),
                FsOperation::Copy,
                FileSystemCapability::DurableCopy,
                "durable-copy contract",
            );
            return;
        }
        let source = self.required_seed("durable-copy-source", b"durable copy", "durable-copy");
        let target = self.path("durable-copy-target");
        let outcome = self
            .fixture
            .file_system()
            .copy(&source, &target, options)
            .expect("durable-copy contract: required-durable copy failed");
        assert!(
            outcome.durable(),
            "durable-copy contract: required operation reported non-durable publication"
        );
        self.assert_bytes(
            &target,
            b"durable copy",
            "durable-copy contract: target bytes mismatch",
        );
        self.context.record_created(source);
        self.context.record_created(target);
    }

    /// Checks temporary resource lifecycle behavior.
    pub fn assert_temp_resources(&mut self) {
        self.context.begin("temp_resources");
        let file_system = self.fixture.file_system();
        if self.capable(FileSystemCapability::TempFile) {
            let parent = self.path("temp-file-parent");
            if self.capable(FileSystemCapability::CreateDirectory) {
                file_system
                    .create_directory(&parent, CreateDirectoryOptions::default())
                    .expect("temp-file contract: parent creation failed");
                self.context.record_created(parent.clone());
            }
            let options = TempFileOptions {
                parent: self
                    .capable(FileSystemCapability::CreateDirectory)
                    .then_some(parent),
                prefix: "contract-file-".to_owned(),
                suffix: ".tmp".to_owned(),
            };
            let mut temporary = file_system
                .create_temp_file(options)
                .expect("temp-file contract: create failed");
            let source = temporary.path().clone();
            assert!(
                source.as_str().contains("/contract-file-"),
                "temp-file contract: requested prefix was ignored"
            );
            assert!(
                source.as_str().ends_with(".tmp"),
                "temp-file contract: requested suffix was ignored"
            );
            temporary
                .cleanup()
                .expect("temp-file contract: cleanup failed");
            assert!(
                !file_system
                    .exists(&source)
                    .expect("temp-file contract: source exists failed"),
                "temp-file contract: cleanup retained source"
            );
            let mut temporary = file_system
                .create_temp_file(Default::default())
                .expect("temp-file contract: persist setup failed");
            let target = self.path("temp-persisted-file");
            self.assert_temp_persist(&mut temporary, &target, "temp-file");
            self.context.record_created(target);
        }
        if self.capable(FileSystemCapability::TempDirectory) {
            let parent = self.path("temp-directory-parent");
            if self.capable(FileSystemCapability::CreateDirectory) {
                file_system
                    .create_directory(&parent, CreateDirectoryOptions::default())
                    .expect("temp-directory contract: parent creation failed");
                self.context.record_created(parent.clone());
            }
            let options = TempDirectoryOptions {
                parent: self
                    .capable(FileSystemCapability::CreateDirectory)
                    .then_some(parent),
                prefix: "contract-directory-".to_owned(),
                suffix: ".tmp".to_owned(),
            };
            let mut temporary = file_system
                .create_temp_directory(options)
                .expect("temp-directory contract: create failed");
            let source = temporary.path().clone();
            assert!(
                source.as_str().contains("/contract-directory-"),
                "temp-directory contract: requested prefix was ignored"
            );
            assert!(
                source.as_str().ends_with(".tmp"),
                "temp-directory contract: requested suffix was ignored"
            );
            temporary
                .cleanup()
                .expect("temp-directory contract: cleanup failed");
            assert!(
                !file_system
                    .exists(&source)
                    .expect("temp-directory contract: source exists failed"),
                "temp-directory contract: cleanup retained source"
            );
            let mut temporary = file_system
                .create_temp_directory(Default::default())
                .expect("temp-directory contract: persist setup failed");
            let target = self.path("temp-persisted-directory");
            self.assert_temp_directory_persist(&mut temporary, &target);
            self.context.record_created(target);
            if self.capable(FileSystemCapability::CreateDirectory) {
                self.assert_temp_directory_overwrite();
            }
        }
    }

    /// Checks structured filesystem error context and redaction behavior.
    pub fn assert_error_context(&mut self) {
        self.context.begin("error_context");
        let path = self.path("error-context-missing");
        let error = self
            .fixture
            .file_system()
            .stat(&path)
            .expect_err("error contract: missing path succeeded");
        self.assert_error(
            &error,
            FsErrorKind::NotFound,
            FsOperation::Stat,
            &path,
            None,
        );
    }

    /// Resolves a fixture path or identifies the contract that could not set
    /// up.
    fn path(&self, relative: &str) -> qubit_fs::Path {
        let relative = self.context.relative_name(relative);
        self.fixture
            .path(&relative)
            .expect("contract: fixture path failed")
    }

    /// Returns whether the immutable snapshot declares a capability.
    fn capable(&self, capability: FileSystemCapability) -> bool {
        self.context
            .properties()
            .capabilities()
            .contains(capability)
    }

    /// Seeds a resource and makes support mandatory for the requested
    /// capability.
    fn required_seed(&self, relative: &str, bytes: &[u8], contract: &str) -> qubit_fs::Path {
        match self.seed(relative, bytes) {
            FixtureSupport::Supported(path) => path,
            FixtureSupport::Unsupported => panic!(
                "{contract} contract: advertised capability requires fixture.seed_file support"
            ),
        }
    }

    /// Delegates out-of-band resource preparation to the fixture.
    fn seed(&self, relative: &str, bytes: &[u8]) -> FixtureSupport<qubit_fs::Path> {
        let relative = self.context.relative_name(relative);
        self.fixture
            .seed_file(&relative, bytes)
            .expect("contract: fixture seed failed")
    }

    /// Reads a provider-owned probe through the fixture and checks exact bytes.
    fn assert_bytes(&self, path: &qubit_fs::Path, expected: &[u8], message: &str) {
        match self
            .fixture
            .read_file(path)
            .expect("copy contract: fixture observation failed")
        {
            FixtureSupport::Supported(actual) => {
                assert_eq!(actual, expected, "{message}")
            }
            FixtureSupport::Unsupported => {
                panic!("copy contract: Copy capability requires fixture.read_file support")
            }
        }
    }

    /// Verifies file persistence publication and the atomic-required preflight.
    fn assert_temp_persist(&self, temporary: &mut TempFile, target: &qubit_fs::Path, label: &str) {
        self.assert_temp_file_persist_result(temporary, target, label);
        if !self.capable(FileSystemCapability::AtomicTempPersist) {
            let mut retry = self
                .fixture
                .file_system()
                .create_temp_file(Default::default())
                .expect("temp-file contract: atomic preflight setup failed");
            let source = retry.path().clone();
            let failure = retry
                .persist(
                    &self.path("temp-required-atomic-file"),
                    PersistOptions::default(),
                )
                .expect_err("temp-file contract: unadvertised required atomic persist succeeded");
            assert_eq!(
                failure.state(),
                PersistFailureState::NotPublished,
                "temp-file contract: failed preflight changed publication responsibility"
            );
            assert_eq!(
                failure.error().kind(),
                FsErrorKind::RequirementNotMet,
                "temp-file contract: failed preflight kind mismatch"
            );
            assert!(
                self.fixture
                    .file_system()
                    .exists(&source)
                    .expect("temp-file contract: source exists failed"),
                "temp-file contract: required atomic preflight removed source"
            );
            retry
                .cleanup()
                .expect("temp-file contract: retained source cleanup failed");
        }
    }

    /// Verifies directory persistence publication and the atomic-required
    /// preflight.
    fn assert_temp_directory_persist(
        &self,
        temporary: &mut TempDirectory,
        target: &qubit_fs::Path,
    ) {
        let outcome = temporary
            .persist(
                target,
                PersistOptions {
                    atomicity: if self.capable(FileSystemCapability::AtomicTempPersist) {
                        AtomicityRequirement::Required
                    } else {
                        AtomicityRequirement::Preferred
                    },
                    ..PersistOptions::default()
                },
            )
            .expect("temp-directory contract: persist failed");
        assert_eq!(
            outcome.target, *target,
            "temp-directory contract: persist target mismatch"
        );
        if self.capable(FileSystemCapability::AtomicTempPersist) {
            assert_eq!(
                outcome.atomicity,
                AchievedAtomicity::Atomic,
                "temp-directory contract: required operation reported non-atomic publication"
            );
        }
        assert!(
            self.fixture
                .file_system()
                .exists(target)
                .expect("temp-directory contract: target exists failed"),
            "temp-directory contract: persist did not publish target"
        );
        if !self.capable(FileSystemCapability::AtomicTempPersist) {
            let mut retry = self
                .fixture
                .file_system()
                .create_temp_directory(Default::default())
                .expect("temp-directory contract: atomic preflight setup failed");
            let source = retry.path().clone();
            let failure = retry
                .persist(
                    &self.path("temp-required-atomic-directory"),
                    PersistOptions::default(),
                )
                .expect_err(
                    "temp-directory contract: unadvertised required atomic persist succeeded",
                );
            assert_eq!(
                failure.state(),
                PersistFailureState::NotPublished,
                "temp-directory contract: failed preflight changed publication responsibility"
            );
            assert_eq!(
                failure.error().kind(),
                FsErrorKind::RequirementNotMet,
                "temp-directory contract: failed preflight kind mismatch"
            );
            assert!(
                self.fixture
                    .file_system()
                    .exists(&source)
                    .expect("temp-directory contract: source exists failed"),
                "temp-directory contract: required atomic preflight removed source"
            );
            retry
                .cleanup()
                .expect("temp-directory contract: retained source cleanup failed");
        }
    }

    /// Verifies a temporary directory replaces an existing empty directory
    /// when the caller explicitly allows replacement.
    fn assert_temp_directory_overwrite(&mut self) {
        let file_system = self.fixture.file_system();
        let target = self.path("temp-overwritten-directory");
        file_system
            .create_directory(&target, CreateDirectoryOptions::default())
            .expect("temp-directory overwrite contract: destination setup failed");
        let mut temporary = file_system
            .create_temp_directory(Default::default())
            .expect("temp-directory overwrite contract: temporary creation failed");
        let outcome = temporary
            .persist(
                &target,
                PersistOptions {
                    overwrite: true,
                    atomicity: if self.capable(FileSystemCapability::AtomicTempPersist) {
                        AtomicityRequirement::Required
                    } else {
                        AtomicityRequirement::Preferred
                    },
                    ..PersistOptions::default()
                },
            )
            .expect("temp-directory overwrite contract: persist failed");
        assert_eq!(
            outcome.target, target,
            "temp-directory overwrite contract: persist target mismatch"
        );
        assert!(
            file_system
                .exists(&target)
                .expect("temp-directory overwrite contract: target exists failed"),
            "temp-directory overwrite contract: replacement did not publish target"
        );
        self.context.record_created(target);
    }

    /// Persists one temporary file using the strongest guarantee it advertises.
    fn assert_temp_file_persist_result(
        &self,
        temporary: &mut TempFile,
        target: &qubit_fs::Path,
        label: &str,
    ) {
        let outcome = temporary
            .persist(
                target,
                PersistOptions {
                    atomicity: if self.capable(FileSystemCapability::AtomicTempPersist) {
                        AtomicityRequirement::Required
                    } else {
                        AtomicityRequirement::Preferred
                    },
                    ..PersistOptions::default()
                },
            )
            .expect("temp-file contract: persist failed");
        assert_eq!(
            outcome.target, *target,
            "{label} contract: persist target mismatch"
        );
        if self.capable(FileSystemCapability::AtomicTempPersist) {
            assert_eq!(
                outcome.atomicity,
                AchievedAtomicity::Atomic,
                "{label} contract: required operation reported non-atomic publication"
            );
        }
        assert!(
            self.fixture
                .file_system()
                .exists(target)
                .expect("temp-file contract: target exists failed"),
            "{label} contract: persist did not publish target"
        );
    }

    /// Validates public error context without exposing provider implementation
    /// details.
    fn assert_error(
        &self,
        error: &qubit_fs::FsError,
        kind: FsErrorKind,
        operation: FsOperation,
        path: &qubit_fs::Path,
        target: Option<&qubit_fs::Path>,
    ) {
        assert_eq!(error.kind(), kind, "error contract: kind mismatch");
        assert_eq!(
            error.operation(),
            operation,
            "error contract: context mismatch"
        );
        assert_eq!(error.path(), Some(path), "error contract: path mismatch");
        assert_eq!(error.target(), target, "error contract: target mismatch");
    }

    /// Validates option-derived capability preflight errors.
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
