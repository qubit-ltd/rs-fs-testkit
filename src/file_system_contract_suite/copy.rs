// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Implements copy contracts.

use super::*;

const COPY_SNAPSHOT_LIMIT: usize = 64 * 1024;

impl<'a> FileSystemContractSuite<'a> {
    /// Checks basic, fallback, native, and stronger copy behavior.
    pub fn assert_copy(&mut self) {
        self.context.begin("copy");
        let source = self.path("copy-source");
        let target = self.path("copy-target");

        if !self.capable(FileSystemCapability::Copy) {
            let error = self
                .fixture
                .file_system()
                .copy(&source, &target, CopyOptions::file())
                .expect_err("copy/basic: unadvertised copy succeeded");
            let required = if !self.capable(FileSystemCapability::Read) {
                FileSystemCapability::Read
            } else if !self.capable(FileSystemCapability::Write) {
                FileSystemCapability::Write
            } else {
                FileSystemCapability::Copy
            };
            if required == FileSystemCapability::Write {
                self.assert_error(
                    error.error(),
                    FsErrorKind::UnsupportedCapability,
                    FsOperation::Copy,
                    &target,
                    Some(&target),
                );
            } else {
                self.assert_error(
                    error.error(),
                    FsErrorKind::UnsupportedCapability,
                    FsOperation::Copy,
                    &source,
                    Some(&target),
                );
            }
            assert_eq!(
                error.error().required_capability(),
                Some(required),
                "copy/basic: missing capability context"
            );
            self.record_copy_rejection();
            return;
        }

        if !self.capable(FileSystemCapability::Read) || !self.capable(FileSystemCapability::Write) {
            let source = self.required_seed("copy-source", b"a", "copy");
            let target = self.path("copy-target");
            let error = self
                .fixture
                .file_system()
                .copy(&source, &target, CopyOptions::file())
                .expect_err("copy/basic: copy without read/write support succeeded");
            let required = if !self.capable(FileSystemCapability::Read) {
                FileSystemCapability::Read
            } else {
                FileSystemCapability::Write
            };
            if required == FileSystemCapability::Write {
                self.assert_error(
                    error.error(),
                    FsErrorKind::UnsupportedCapability,
                    FsOperation::Copy,
                    &target,
                    Some(&target),
                );
            } else {
                self.assert_error(
                    error.error(),
                    FsErrorKind::UnsupportedCapability,
                    FsOperation::Copy,
                    &source,
                    Some(&target),
                );
            }
            assert_eq!(
                error.error().required_capability(),
                Some(required),
                "copy/basic: facade reported the wrong missing capability"
            );
            self.record_copy_rejection();
            return;
        }

        let source = self.required_seed("copy-source", b"a", "copy");
        let target = self.path("copy-target");
        self.context.record_created(target.clone());
        let outcome = self
            .fixture
            .file_system()
            .copy(&source, &target, CopyOptions::file())
            .expect("copy/basic: copy failed");
        assert_eq!(outcome.stats().bytes, 1, "copy/basic: byte count mismatch");
        assert_eq!(
            outcome.stats().files + outcome.stats().objects,
            1,
            "copy/basic: copied resource count mismatch"
        );
        self.assert_bytes(&source, b"a", "copy/basic: source was modified");
        self.assert_bytes(&target, b"a", "copy/basic: target bytes mismatch");
        match (self.fixture.copy_fallback_only(), outcome.method()) {
            (true, CopyMethod::Streamed) => assert!(
                outcome.used_fallback(),
                "copy/basic: streamed copy did not report fallback"
            ),
            (true, _) => panic!("copy/basic: fallback-only fixture used a native method"),
            (false, CopyMethod::Streamed) => assert!(
                outcome.used_fallback(),
                "copy/basic: streamed copy did not report fallback"
            ),
            (false, _) => assert!(
                !outcome.used_fallback(),
                "copy/basic: native copy was reported as fallback"
            ),
        }
        self.context.record_check(
            "copy/basic",
            Some(FileSystemCapability::Copy),
            ContractCheckOutcome::Passed,
        );
        self.assert_copy_conflicts(&source);
        self.assert_copy_tree();
        self.assert_server_side_copy();
        self.assert_atomic_copy(CopyMode::File);
    }

    fn record_copy_rejection(&mut self) {
        self.context.record_check(
            "copy/basic",
            Some(FileSystemCapability::Copy),
            ContractCheckOutcome::RejectedAsExpected,
        );
        self.context.record_check(
            "copy/fallback-overwrite-rejected",
            Some(FileSystemCapability::Copy),
            ContractCheckOutcome::NotApplicable {
                reason: "basic copy is unavailable".to_owned(),
            },
        );
        for (id, capability) in [
            ("copy/server-side", FileSystemCapability::ServerSideCopy),
            ("copy/atomic-file", FileSystemCapability::AtomicFileCopy),
            ("copy/atomic-tree", FileSystemCapability::AtomicTreeCopy),
        ] {
            self.context.record_check(
                id,
                Some(capability),
                ContractCheckOutcome::NotApplicable {
                    reason: "basic copy is unavailable".to_owned(),
                },
            );
        }
    }

    /// Checks destination conflict policies and copy statistics.
    pub fn assert_copy_conflicts(&mut self, source: &Path) {
        let target = self.required_seed("copy-conflict-target", b"b", "copy-conflict");
        let failure = self
            .fixture
            .file_system()
            .copy(source, &target, CopyOptions::file())
            .expect_err("copy/fallback-overwrite-rejected: default conflict replaced target");
        self.assert_error(
            failure.error(),
            FsErrorKind::AlreadyExists,
            FsOperation::Copy,
            source,
            Some(&target),
        );
        self.assert_bytes(
            &target,
            b"b",
            "copy/fallback-overwrite-rejected: target changed",
        );

        let skipped = self
            .fixture
            .file_system()
            .copy(
                source,
                &target,
                CopyOptions::file().with_conflict(CopyConflictPolicy::Skip),
            )
            .expect("copy/fallback-overwrite-rejected: skip conflict failed");
        assert_eq!(skipped.stats().skipped, 1);
        self.assert_bytes(
            &target,
            b"b",
            "copy/fallback-overwrite-rejected: skip changed target",
        );

        let overwrite = self.fixture.file_system().copy(
            source,
            &target,
            CopyOptions::file().with_conflict(CopyConflictPolicy::Overwrite),
        );
        if self.fixture.copy_fallback_only() {
            let failure = overwrite.expect_err(
                "copy/fallback-overwrite-rejected: fallback unexpectedly accepted overwrite",
            );
            assert_eq!(failure.error().kind(), FsErrorKind::RequirementNotMet);
            assert_eq!(failure.error().operation(), FsOperation::Copy);
            self.assert_bytes(
                &target,
                b"b",
                "copy/fallback-overwrite-rejected: failed overwrite changed target",
            );
            self.context.record_check(
                "copy/fallback-overwrite-rejected",
                Some(FileSystemCapability::Copy),
                ContractCheckOutcome::RejectedAsExpected,
            );
        } else {
            let outcome = overwrite.expect("copy/fallback-overwrite-rejected: overwrite failed");
            assert_eq!(outcome.stats().overwritten, 1);
            self.assert_bytes(
                &target,
                b"a",
                "copy/fallback-overwrite-rejected: overwrite bytes mismatch",
            );
            self.context.record_check(
                "copy/fallback-overwrite-rejected",
                Some(FileSystemCapability::Copy),
                ContractCheckOutcome::Passed,
            );
        }
    }

    fn assert_copy_tree(&mut self) {
        if !self.capable(FileSystemCapability::Copy)
            || !self.capable(FileSystemCapability::CreateDirectory)
        {
            self.context.record_check(
                "copy/atomic-tree",
                Some(FileSystemCapability::AtomicTreeCopy),
                ContractCheckOutcome::NotApplicable {
                    reason: "tree fixture requires Copy and CreateDirectory".to_owned(),
                },
            );
            return;
        }
        if self.fixture.copy_fallback_only() {
            let source = self.path("copy-tree-source");
            let target = self.path("copy-tree-target");
            self.fixture
                .file_system()
                .create_directory(&source, CreateDirectoryOptions::default())
                .expect("copy/atomic-tree: fallback source setup failed");
            self.context.record_created(source.clone());
            let _child = self.required_seed("copy-tree-source/child", b"a", "copy/atomic-tree");
            let failure = self
                .fixture
                .file_system()
                .copy(&source, &target, CopyOptions::tree())
                .expect_err("copy/atomic-tree: fallback unexpectedly accepted tree copy");
            assert_eq!(failure.error().kind(), FsErrorKind::InvalidOptions);
            assert_eq!(failure.error().operation(), FsOperation::Copy);
            self.context.record_check(
                "copy/atomic-tree",
                Some(FileSystemCapability::AtomicTreeCopy),
                ContractCheckOutcome::RejectedAsExpected,
            );
            return;
        }
        let root = self.path("copy-tree-source");
        let sub = self.path("copy-tree-source/sub");
        let target = self.path("copy-tree-target");
        self.fixture
            .file_system()
            .create_directory(&root, CreateDirectoryOptions::default())
            .expect("copy/atomic-tree: source root setup failed");
        self.context.record_created(root.clone());
        self.fixture
            .file_system()
            .create_directory(&sub, CreateDirectoryOptions::default())
            .expect("copy/atomic-tree: source subdirectory setup failed");
        self.context.record_created(sub);
        let child = self.required_seed("copy-tree-source/sub/b", b"b", "copy/atomic-tree");
        let outcome = self
            .fixture
            .file_system()
            .copy(&root, &target, CopyOptions::tree())
            .expect("copy/atomic-tree: tree copy failed");
        self.context.record_created(target.clone());
        let target_child = self.path("copy-tree-target/sub/b");
        self.assert_bytes(
            &target_child,
            b"b",
            "copy/atomic-tree: child bytes mismatch",
        );
        assert!(
            self.fixture
                .file_system()
                .exists(&root)
                .expect("copy/atomic-tree: source check failed")
        );
        assert!(
            self.fixture
                .file_system()
                .exists(&child)
                .expect("copy/atomic-tree: child check failed")
        );
        assert!(outcome.stats().files + outcome.stats().objects >= 1);
        if self.capable(FileSystemCapability::AtomicTreeCopy) {
            let atomic_target = self.path("copy-tree-atomic-target");
            self.context.record_created(atomic_target.clone());
            let atomic = self
                .fixture
                .file_system()
                .copy(
                    &root,
                    &atomic_target,
                    CopyOptions::tree().with_atomicity(AtomicityRequirement::Required),
                )
                .expect("copy/atomic-tree: required atomic tree copy failed");
            assert_eq!(
                atomic.atomicity(),
                AchievedAtomicity::Atomic,
                "copy/atomic-tree: non-atomic result"
            );
            let atomic_child = self.path("copy-tree-atomic-target/sub/b");
            self.assert_bytes(
                &atomic_child,
                b"b",
                "copy/atomic-tree: atomic child mismatch",
            );
        }
        self.context.record_check(
            "copy/atomic-tree",
            Some(FileSystemCapability::AtomicTreeCopy),
            ContractCheckOutcome::Passed,
        );
    }

    fn assert_server_side_copy(&mut self) {
        if !self.capable(FileSystemCapability::ServerSideCopy) {
            let source = self.path("copy-server-side-unavailable-source");
            let target = self.path("copy-server-side-unavailable-target");
            let failure = self
                .fixture
                .file_system()
                .copy(
                    &source,
                    &target,
                    CopyOptions::file().with_server_side(ServerSidePreference::Require),
                )
                .expect_err("copy/server-side: unadvertised server-side copy succeeded");
            self.assert_requirement_error(
                failure.error(),
                FsOperation::Copy,
                FileSystemCapability::ServerSideCopy,
                "copy/server-side",
            );
            self.context.record_check(
                "copy/server-side",
                Some(FileSystemCapability::ServerSideCopy),
                ContractCheckOutcome::RejectedAsExpected,
            );
            return;
        }
        let case = self
            .fixture
            .copy_fast_path_case(CopyMethod::ServerSide)
            .expect("copy/server-side: fixture fast-path setup failed");
        let FixtureSupport::Supported(case) = case else {
            self.context.record_check(
                "copy/server-side",
                Some(FileSystemCapability::ServerSideCopy),
                ContractCheckOutcome::Unverified {
                    reason: "fixture has no server-side copy case".to_owned(),
                },
            );
            return;
        };
        let snapshot = match self
            .fixture
            .read_file(case.source())
            .expect("copy/server-side: source snapshot failed")
        {
            FixtureSupport::Supported(bytes) if bytes.len() <= COPY_SNAPSHOT_LIMIT => bytes,
            FixtureSupport::Supported(bytes) => panic!(
                "copy/server-side: fixture/copy-case-too-large ({})",
                bytes.len()
            ),
            FixtureSupport::Unsupported => {
                panic!("copy/server-side: fixture must independently observe the source")
            }
        };
        self.context.record_created(case.source().clone());
        self.context.record_created(case.target().clone());
        let outcome = self
            .fixture
            .file_system()
            .copy(case.source(), case.target(), case.options().clone())
            .expect("copy/server-side: native case failed");
        assert_eq!(
            outcome.method(),
            CopyMethod::ServerSide,
            "copy/server-side: method mismatch"
        );
        assert!(
            !outcome.used_fallback(),
            "copy/server-side: unexpectedly used fallback"
        );
        assert_eq!(outcome.stats().bytes, snapshot.len() as u64);
        self.assert_bytes(case.source(), &snapshot, "copy/server-side: source changed");
        self.assert_bytes(
            case.target(),
            &snapshot,
            "copy/server-side: target mismatch",
        );
        self.context.record_check(
            "copy/server-side",
            Some(FileSystemCapability::ServerSideCopy),
            ContractCheckOutcome::Passed,
        );
    }

    fn assert_atomic_copy(&mut self, mode: CopyMode) {
        let (id, capability, relative) = match mode {
            CopyMode::File => (
                "copy/atomic-file",
                FileSystemCapability::AtomicFileCopy,
                "atomic-copy-file",
            ),
            CopyMode::Tree => (
                "copy/atomic-tree",
                FileSystemCapability::AtomicTreeCopy,
                "atomic-copy-tree",
            ),
            CopyMode::Auto => return,
        };
        if !self.capable(capability) {
            let source = self.path(&format!("{relative}-source"));
            let target = self.path(&format!("{relative}-target"));
            let failure = self
                .fixture
                .file_system()
                .copy(
                    &source,
                    &target,
                    CopyOptions::default()
                        .with_mode(mode)
                        .with_atomicity(AtomicityRequirement::Required),
                )
                .expect_err("copy/atomic: unadvertised required atomic copy succeeded");
            self.assert_requirement_error(&failure.error(), FsOperation::Copy, capability, id);
            self.context.record_check(
                id,
                Some(capability),
                ContractCheckOutcome::RejectedAsExpected,
            );
            return;
        }
        if mode == CopyMode::Tree {
            return;
        }
        let source = self.required_seed(&format!("{relative}-source"), b"a", id);
        let target = self.path(&format!("{relative}-target"));
        self.context.record_created(target.clone());
        let outcome = self
            .fixture
            .file_system()
            .copy(
                &source,
                &target,
                CopyOptions::default()
                    .with_mode(mode)
                    .with_atomicity(AtomicityRequirement::Required),
            )
            .unwrap_or_else(|_| panic!("{id}: required atomic copy failed"));
        assert_eq!(
            outcome.atomicity(),
            AchievedAtomicity::Atomic,
            "{id}: non-atomic result"
        );
        self.assert_bytes(&source, b"a", &format!("{id}: source changed"));
        self.assert_bytes(&target, b"a", &format!("{id}: target mismatch"));
        self.context
            .record_check(id, Some(capability), ContractCheckOutcome::Passed);
    }

    /// Checks required-durable copy publication when advertised.
    pub fn assert_durable_copy(&mut self) {
        self.context.begin("durable_copy");
        let source = self.path("durable-copy-source");
        let target = self.path("durable-copy-target");
        if !self.capable(FileSystemCapability::DurableFileCopy) {
            let failure = self
                .fixture
                .file_system()
                .copy(
                    &source,
                    &target,
                    CopyOptions::file().with_durability(DurabilityRequirement::Required),
                )
                .expect_err("copy/durable-file: unadvertised durable copy succeeded");
            self.assert_requirement_error(
                failure.error(),
                FsOperation::Copy,
                FileSystemCapability::DurableFileCopy,
                "copy/durable-file",
            );
            self.context.record_check(
                "copy/durable-file",
                Some(FileSystemCapability::DurableFileCopy),
                ContractCheckOutcome::RejectedAsExpected,
            );
            self.context.record_check(
                "copy/durable-tree",
                Some(FileSystemCapability::DurableTreeCopy),
                ContractCheckOutcome::RejectedAsExpected,
            );
            return;
        }
        let source = self.required_seed("durable-copy-source", b"a", "copy/durable-file");
        let target = self.path("durable-copy-target");
        self.context.record_created(target.clone());
        let outcome = self
            .fixture
            .file_system()
            .copy(
                &source,
                &target,
                CopyOptions::file().with_durability(DurabilityRequirement::Required),
            )
            .expect("copy/durable-file: required durable copy failed");
        assert!(
            outcome.durable(),
            "copy/durable-file: result was not durable"
        );
        self.assert_bytes(&target, b"a", "copy/durable-file: target mismatch");
        self.context.record_check(
            "copy/durable-file",
            Some(FileSystemCapability::DurableFileCopy),
            ContractCheckOutcome::Passed,
        );
        self.context.record_check(
            "copy/durable-tree",
            Some(FileSystemCapability::DurableTreeCopy),
            ContractCheckOutcome::NotApplicable {
                reason: "durable tree requires a dedicated tree fixture".to_owned(),
            },
        );
    }
}
