// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements namespace and metadata contracts.

use super::*;

impl<'a> AsyncFileSystemContractSuite<'a> {
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
        self.assert_error(&error, FsErrorKind::NotFound, FsOperation::Stat, &path);
        self.context.record_check(
            "stat/basic",
            Some(FileSystemCapability::Read),
            ContractCheckOutcome::Passed,
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
                "stat/file-kind: seeded resource is not file-like"
            );
            assert_eq!(
                metadata.len(),
                Some(13),
                "stat contract: file length mismatch"
            );
            self.context.record_check(
                "stat/file-kind",
                None,
                ContractCheckOutcome::Passed,
            );
        } else {
            self.context.record_check(
                "stat/file-kind",
                None,
                ContractCheckOutcome::Unverified {
                    reason: "fixture cannot seed a stat resource".to_owned(),
                },
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
                .expect("list/basic: stream error")
            {
                actual.push(entry.path);
            }
            actual.sort_by(|left, right| left.as_str().cmp(right.as_str()));
            let mut expected = vec![first, second];
            expected.sort_by(|left, right| left.as_str().cmp(right.as_str()));
            assert_eq!(actual, expected, "list/basic: direct children mismatch");
            self.context.record_check(
                "list/basic",
                Some(FileSystemCapability::List),
                ContractCheckOutcome::Passed,
            );
            let nested = self
                .required_seed("async-list/prefixed/nested", b"nested", "list")
                .await;
            let nested_second = self
                .required_seed("async-list/prefixed/second", b"second nested", "list")
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
                    ListOptions::default()
                        .with_include_metadata(true)
                        .with_prefix(Some(prefix))
                        .with_page_size(Some(1)),
                )
                .await
                .expect("list contract: prefix listing failed");
            let mut prefixed = Vec::new();
            while let Some(entry) = stream
                .next_entry_async()
                .await
                .expect("list/prefix: prefix stream error")
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
            self.context.record_check(
                "list/prefix",
                Some(FileSystemCapability::List),
                ContractCheckOutcome::Passed,
            );
            self.context.record_check(
                "list/pagination",
                Some(FileSystemCapability::List),
                ContractCheckOutcome::Passed,
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
            for id in ["list/basic", "list/prefix", "list/pagination"] {
                self.context.record_check(
                    id,
                    Some(FileSystemCapability::List),
                    ContractCheckOutcome::RejectedAsExpected,
                );
            }
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
                .expect("create-directory contract: advertised creation failed");
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
                    CreateDirectoryOptions::default().with_exists_ok(true),
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
                    CreateDirectoryOptions::default().with_recursive(true),
                )
                .await
                .expect("create-directory contract: recursive creation failed");
            if let Some(created_ancestors) = outcome.created_ancestors() {
                assert!(created_ancestors > 0);
            }
            self.context.record_check(
                "directory/create",
                Some(FileSystemCapability::CreateDirectory),
                ContractCheckOutcome::Passed,
            );
            self.context.record_check(
                "directory/recursive",
                Some(FileSystemCapability::CreateDirectory),
                ContractCheckOutcome::Passed,
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
            self.context.record_check(
                "directory/create",
                Some(FileSystemCapability::CreateDirectory),
                ContractCheckOutcome::RejectedAsExpected,
            );
            self.context.record_check(
                "directory/recursive",
                Some(FileSystemCapability::CreateDirectory),
                ContractCheckOutcome::RejectedAsExpected,
            );
        }
    }

    /// Checks advertised asynchronous empty-directory and symlink
    /// representations.
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
            self.context.record_check(
                "representation/empty",
                Some(FileSystemCapability::EmptyDirectory),
                ContractCheckOutcome::Passed,
            );
        } else {
            self.context.record_check(
                "representation/empty",
                Some(FileSystemCapability::EmptyDirectory),
                ContractCheckOutcome::RejectedAsExpected,
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
                FixtureSupport::Unsupported => {
                    panic!("representation contract: Symlink requires fixture.seed_symlink support")
                }
            };
            self.context.record_created(path.clone());
            let metadata = self
                .fixture
                .file_system()
                .stat(&path)
                .await
                .expect("representation contract: symlink is not statable");
            assert_eq!(
                metadata.kind(),
                &FileKind::Symlink,
                "representation contract: seeded link kind mismatch"
            );
            self.context.record_check(
                "representation/symlink",
                Some(FileSystemCapability::Symlink),
                ContractCheckOutcome::Passed,
            );
        } else {
            self.context.record_check(
                "representation/symlink",
                Some(FileSystemCapability::Symlink),
                ContractCheckOutcome::RejectedAsExpected,
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
            self.assert_error(&error, FsErrorKind::NotFound, FsOperation::Stat, &path);
            self.assert_delete_options().await;
            self.context.record_check(
                "delete/basic",
                Some(FileSystemCapability::Delete),
                ContractCheckOutcome::Passed,
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
            self.context.record_check(
                "delete/basic",
                Some(FileSystemCapability::Delete),
                ContractCheckOutcome::RejectedAsExpected,
            );
            self.context.record_check(
                "delete/missing-ok",
                Some(FileSystemCapability::Delete),
                ContractCheckOutcome::NotApplicable {
                    reason: "delete is unavailable".to_owned(),
                },
            );
            self.context.record_check(
                "delete/if-match",
                Some(FileSystemCapability::ConditionalDelete),
                ContractCheckOutcome::NotApplicable {
                    reason: "delete is unavailable".to_owned(),
                },
            );
        }
    }

    /// Checks asynchronous missing-ok and conditional deletion semantics.
    pub async fn assert_delete_options(&mut self) {
        let missing = self.path("async-delete-missing-ok");
        let outcome = self
            .fixture
            .file_system()
            .delete_file(&missing, DeleteOptions::default().with_missing_ok(true))
            .await
            .expect("delete contract: missing-ok deletion failed");
        assert!(
            outcome.already_missing(),
            "delete contract: missing-ok outcome did not report absence"
        );
        self.context.record_check(
            "delete/missing-ok",
            Some(FileSystemCapability::Delete),
            ContractCheckOutcome::Passed,
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
                .delete_file(&path, DeleteOptions::default().with_if_match(Some(version)))
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
            self.context.record_check(
                "delete/if-match",
                Some(FileSystemCapability::ConditionalDelete),
                ContractCheckOutcome::Passed,
            );
        } else {
            let error = self
                .fixture
                .file_system()
                .delete_file(
                    &path,
                    DeleteOptions::default()
                        .with_if_match(Some(ResourceVersion::new("contract-version"))),
                )
                .await
                .expect_err("delete contract: unadvertised conditional delete succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::Delete,
                FileSystemCapability::ConditionalDelete,
                "conditional-delete contract",
            );
            self.context.record_check(
                "delete/if-match",
                Some(FileSystemCapability::ConditionalDelete),
                ContractCheckOutcome::RejectedAsExpected,
            );
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
            self.assert_rename_conflicts().await;
            self.context.record_check(
                "rename/basic",
                Some(FileSystemCapability::Rename),
                ContractCheckOutcome::Passed,
            );
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
            self.context.record_check(
                "rename/basic",
                Some(FileSystemCapability::Rename),
                ContractCheckOutcome::RejectedAsExpected,
            );
            self.context.record_check(
                "rename/conflict",
                Some(FileSystemCapability::Rename),
                ContractCheckOutcome::NotApplicable {
                    reason: "rename is unavailable".to_owned(),
                },
            );
        }
    }

    /// Checks asynchronous rename conflicts and explicit overwrite.
    pub async fn assert_rename_conflicts(&mut self) {
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
        assert_error_with_source_or_target(
            failure.error(),
            FsErrorKind::AlreadyExists,
            FsOperation::Rename,
            &source,
            &target,
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
                RenameOptions::default().with_overwrite(true),
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
        self.context.record_check(
            "rename/conflict",
            Some(FileSystemCapability::Rename),
            ContractCheckOutcome::Passed,
        );
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
        let options = DeleteOptions::default().with_recursive(true);
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
            self.context.record_check(
                "delete/tree",
                Some(FileSystemCapability::RecursiveDelete),
                ContractCheckOutcome::RejectedAsExpected,
            );
            return;
        }
        if self.capable(FileSystemCapability::CreateDirectory) {
            self.fixture
                .file_system()
                .create_directory(&root, CreateDirectoryOptions::default())
                .await
                .expect("recursive-delete contract: root creation failed");
            self.context.record_created(root.clone());
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
            !self
                .fixture
                .file_system()
                .exists(&root)
                .await
                .expect("recursive-delete contract: root existence check failed")
        );
        assert!(
            !self
                .fixture
                .file_system()
                .exists(&child)
                .await
                .expect("recursive-delete contract: child existence check failed")
        );
        self.context.record_check(
            "delete/tree",
            Some(FileSystemCapability::RecursiveDelete),
            ContractCheckOutcome::Passed,
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
        let options = RenameOptions::default().with_atomicity(AtomicityRequirement::Required);
        if !self.capable(FileSystemCapability::AtomicRename) {
            let failure = self
                .fixture
                .file_system()
                .rename(&source, &target, options)
                .await
                .expect_err("atomic-rename contract: unadvertised preflight succeeded");
            self.assert_requirement_error(
                failure.error(),
                FsOperation::Rename,
                FileSystemCapability::AtomicRename,
                "atomic-rename contract",
            );
            self.context.record_check(
                "rename/atomic",
                Some(FileSystemCapability::AtomicRename),
                ContractCheckOutcome::RejectedAsExpected,
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
        self.context.record_check(
            "rename/atomic",
            Some(FileSystemCapability::AtomicRename),
            ContractCheckOutcome::Passed,
        );
    }

    /// Checks required-durable rename publication when advertised.
    pub async fn assert_durable_rename(&mut self) {
        self.context.begin("durable_rename");
        let source = self.path("async-durable-rename-source");
        let target = self.path("async-durable-rename-target");
        let options = RenameOptions::default().with_durability(DurabilityRequirement::Required);
        if !self.capable(FileSystemCapability::DurableRename) {
            let failure = self
                .fixture
                .file_system()
                .rename(&source, &target, options)
                .await
                .expect_err("durable-rename contract: unadvertised preflight succeeded");
            self.assert_requirement_error(
                failure.error(),
                FsOperation::Rename,
                FileSystemCapability::DurableRename,
                "durable-rename contract",
            );
            self.context.record_check(
                "rename/durable",
                Some(FileSystemCapability::DurableRename),
                ContractCheckOutcome::RejectedAsExpected,
            );
            return;
        }
        let source = self
            .required_seed(
                "async-durable-rename-source",
                b"durable rename",
                "durable-rename",
            )
            .await;
        self.context.record_created(target.clone());
        let outcome = self
            .fixture
            .file_system()
            .rename(&source, &target, options)
            .await
            .expect("durable-rename contract: required rename failed");
        assert!(
            outcome.durable(),
            "durable-rename contract: required operation reported non-durable publication"
        );
        self.assert_bytes(
            &target,
            b"durable rename",
            "durable-rename contract: target bytes mismatch",
        )
        .await;
        self.context.record_check(
            "rename/durable",
            Some(FileSystemCapability::DurableRename),
            ContractCheckOutcome::Passed,
        );
    }
}
