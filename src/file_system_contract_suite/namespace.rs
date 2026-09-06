// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements namespace and metadata contracts.

use super::*;

impl<'a> FileSystemContractSuite<'a> {
    /// Checks metadata behavior.
    ///
    /// # Panics
    ///
    /// Panics when missing-path errors are not structured correctly or seeded
    /// file metadata does not match the fixture content.
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
            assert!(
                metadata.is_file_like(),
                "stat contract: seeded resource is not file-like"
            );
            assert_eq!(
                metadata.len(),
                Some(13),
                "stat contract: file length mismatch"
            );
        }
    }

    /// Checks directory listing behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, child enumeration, prefix filtering,
    /// pagination, or requested metadata violates the listing contract.
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
        let nested_second =
            self.required_seed("list-root/prefixed/second", b"second nested", "list");
        self.context.record_created(nested_second.clone());
        let prefix = self
            .fixture
            .list_prefix(&root, "prefixed")
            .expect("list contract: fixture prefix failed");
        let mut stream = self
            .fixture
            .file_system()
            .list(
                &root,
                ListOptions::default()
                    .with_include_metadata(true)
                    .with_prefix(Some(prefix))
                    .with_page_size(Some(1)),
            )
            .expect("list contract: prefix listing failed");
        let mut prefixed = Vec::new();
        while let Some(entry) = stream
            .next_entry()
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
        let prefix_entry = Path::parse(
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
    }

    /// Checks directory creation behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, directory publication, metadata, or
    /// existing-directory handling violates the creation contract.
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
        self.context.record_created(path.clone());
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
                CreateDirectoryOptions::default().with_exists_ok(true),
            )
            .expect("namespace contract: existing directory was not accepted");
        assert!(
            outcome.already_existed(),
            "namespace contract: existing directory outcome was not reported"
        );
        let parent = self.path("created-recursive-parent");
        let child = self.path("created-recursive-parent/child");
        self.context.record_created(parent);
        self.context.record_created(child.clone());
        let outcome = self
            .fixture
            .file_system()
            .create_directory(
                &child,
                CreateDirectoryOptions::default().with_recursive(true),
            )
            .expect("namespace contract: recursive directory creation failed");
        if let Some(created_ancestors) = outcome.created_ancestors() {
            assert!(
                created_ancestors > 0,
                "namespace contract: recursive ancestor count is zero"
            );
        }
    }

    /// Checks advertised empty-directory and symbolic-link representations.
    pub fn assert_representations(&mut self) {
        self.context.begin("representations");
        if self.capable(FileSystemCapability::EmptyDirectory) {
            let relative = self.context.relative_name("empty-directory");
            let path = match self
                .fixture
                .seed_empty_directory(&relative)
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
                .expect("representation contract: symlink setup failed")
            {
                FixtureSupport::Supported(path) => path,
                FixtureSupport::Unsupported => {
                    panic!("representation contract: Symlink requires fixture.seed_symlink support")
                }
            };
            self.context.record_created(path.clone());
            let metadata = self
                .fixture
                .file_system()
                .stat(&path)
                .expect("representation contract: symlink is not statable");
            assert_eq!(
                metadata.kind(),
                &FileKind::Symlink,
                "representation contract: seeded link kind mismatch"
            );
        }
    }

    /// Checks deletion behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, deletion, existence observation, or
    /// structured error context violates the deletion contract.
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
        let outcome = self
            .fixture
            .file_system()
            .delete_file(&path, DeleteOptions::default())
            .expect("delete contract: deletion failed");
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
        assert!(
            !self
                .fixture
                .file_system()
                .exists(&path)
                .expect("delete contract: exists failed"),
            "delete contract: deleted file remained"
        );
        self.assert_delete_options();
    }

    /// Checks missing-ok and conditional deletion semantics.
    pub fn assert_delete_options(&mut self) {
        let missing = self.path("delete-missing-ok");
        let outcome = self
            .fixture
            .file_system()
            .delete_file(&missing, DeleteOptions::default().with_missing_ok(true))
            .expect("delete contract: missing-ok deletion failed");
        assert!(
            outcome.already_missing(),
            "delete contract: missing-ok outcome did not report absence"
        );

        let path = self.path("delete-conditional");
        let options =
            DeleteOptions::default().with_if_match(Some(ResourceVersion::new("contract-version")));
        if self.capable(FileSystemCapability::ConditionalDelete) {
            let path = self.required_seed(
                "delete-conditional",
                b"conditional delete",
                "conditional-delete",
            );
            self.context.record_created(path.clone());
            let version = match self
                .fixture
                .resource_version(&path)
                .expect("delete contract: version observation failed")
            {
                FixtureSupport::Supported(version) => version,
                FixtureSupport::Unsupported => panic!(
                    "conditional-delete contract: advertised capability requires fixture.resource_version support"
                ),
            };
            self.fixture
                .file_system()
                .delete_file(&path, DeleteOptions::default().with_if_match(Some(version)))
                .expect("delete contract: advertised conditional delete failed");
            assert!(
                !self
                    .fixture
                    .file_system()
                    .exists(&path)
                    .expect("delete contract: conditional target observation failed")
            );
        } else {
            let error = self
                .fixture
                .file_system()
                .delete_file(&path, options)
                .expect_err("delete contract: unadvertised conditional delete succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::Delete,
                FileSystemCapability::ConditionalDelete,
                "conditional-delete contract",
            );
        }
    }

    /// Checks rename behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, publication, source removal, target
    /// content, or structured error context violates the rename contract.
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
        self.context.record_created(target.clone());
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
        self.assert_rename_conflicts();
    }

    /// Checks rename destination conflicts and explicit overwrite.
    pub fn assert_rename_conflicts(&mut self) {
        let source = self.required_seed(
            "rename-conflict-source",
            b"rename source",
            "rename-conflict",
        );
        let target = self.required_seed(
            "rename-conflict-target",
            b"rename target",
            "rename-conflict",
        );
        self.context.record_created(source.clone());
        self.context.record_created(target.clone());
        let failure = self
            .fixture
            .file_system()
            .rename(&source, &target, RenameOptions::default())
            .expect_err("rename contract: default conflict replaced target");
        assert_eq!(
            failure.state(),
            RenameFailureState::Unchanged,
            "rename contract: conflict failure state mismatch"
        );
        self.assert_error(
            failure.error(),
            FsErrorKind::AlreadyExists,
            FsOperation::Rename,
            &source,
            Some(&target),
        );
        self.assert_bytes(
            &source,
            b"rename source",
            "rename contract: conflict removed source",
        );
        self.assert_bytes(
            &target,
            b"rename target",
            "rename contract: conflict changed target",
        );

        self.fixture
            .file_system()
            .rename(
                &source,
                &target,
                RenameOptions::default().with_overwrite(true),
            )
            .expect("rename contract: overwrite failed");
        assert!(
            !self
                .fixture
                .file_system()
                .exists(&source)
                .expect("rename contract: overwrite source observation failed")
        );
        self.assert_bytes(
            &target,
            b"rename source",
            "rename contract: overwrite target mismatch",
        );
    }

    /// Checks recursive directory removal when the provider advertises it.
    ///
    /// # Panics
    ///
    /// Panics when recursive-delete preflight or descendant removal violates
    /// the advertised capability, or fixture setup fails.
    pub fn assert_recursive_delete(&mut self) {
        self.context.begin("recursive_delete");
        let root = self.path("recursive-delete-root");
        let options = DeleteOptions::default().with_recursive(true);
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
        if self.capable(FileSystemCapability::CreateDirectory) {
            self.fixture
                .file_system()
                .create_directory(&root, CreateDirectoryOptions::default())
                .expect("recursive-delete contract: root creation failed");
            self.context.record_created(root.clone());
        }
        let child = self.required_seed("recursive-delete-root/child", b"child", "recursive-delete");
        self.context.record_created(child.clone());
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
    ///
    /// # Panics
    ///
    /// Panics when atomic-rename preflight, publication, or reported atomicity
    /// violates the advertised capability.
    pub fn assert_atomic_rename(&mut self) {
        self.context.begin("atomic_rename");
        let source = self.path("atomic-rename-source");
        let target = self.path("atomic-rename-target");
        let options = RenameOptions::default().with_atomicity(AtomicityRequirement::Required);
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
        self.context.record_created(source.clone());
        self.context.record_created(target.clone());
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
    }

    /// Checks required-durable rename publication when advertised.
    pub fn assert_durable_rename(&mut self) {
        self.context.begin("durable_rename");
        let source = self.path("durable-rename-source");
        let target = self.path("durable-rename-target");
        let options = RenameOptions::default().with_durability(DurabilityRequirement::Required);
        if !self.capable(FileSystemCapability::DurableRename) {
            let failure = self
                .fixture
                .file_system()
                .rename(&source, &target, options)
                .expect_err("durable-rename contract: unadvertised preflight succeeded");
            self.assert_requirement_error(
                failure.error(),
                FsOperation::Rename,
                FileSystemCapability::DurableRename,
                "durable-rename contract",
            );
            return;
        }
        let source =
            self.required_seed("durable-rename-source", b"durable rename", "durable-rename");
        self.context.record_created(source.clone());
        self.context.record_created(target.clone());
        let outcome = self
            .fixture
            .file_system()
            .rename(&source, &target, options)
            .expect("durable-rename contract: required rename failed");
        assert!(
            outcome.durable(),
            "durable-rename contract: required operation reported non-durable publication"
        );
        self.assert_bytes(
            &target,
            b"durable rename",
            "durable-rename contract: target bytes mismatch",
        );
    }
}
