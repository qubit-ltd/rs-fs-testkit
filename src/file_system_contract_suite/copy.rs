// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements copy contracts.

use super::*;

impl<'a> FileSystemContractSuite<'a> {
    /// Checks native and fallback copy behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, copy publication, method reporting,
    /// directory recursion, or a required native fixture case is invalid.
    pub fn assert_copy(&mut self) {
        self.context.begin("copy");
        if !self.capable(FileSystemCapability::Copy) {
            let source = self.path("copy-unavailable");
            let target = self.path("copy-unavailable-target");
            let error = self
                .fixture
                .file_system()
                .copy(&source, &target, CopyOptions::default())
                .expect_err("copy contract: unavailable fallback succeeded");
            self.assert_error(
                error.error(),
                FsErrorKind::UnsupportedCapability,
                FsOperation::Copy,
                &source,
                Some(&target),
            );
            assert_eq!(
                error.error().required_capability(),
                Some(FileSystemCapability::Read),
                "copy contract: fallback missing read-capability context"
            );
            return;
        }
        let source = self.required_seed("copy-source", b"copy bytes", "copy");
        self.context.record_created(source.clone());
        let target = self.path("copy-target");
        self.context.record_created(target.clone());
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
        assert_eq!(outcome.stats().bytes, 10, "copy contract: copied byte count mismatch");
        assert_eq!(
            outcome.stats().files + outcome.stats().objects,
            1,
            "copy contract: copied resource count mismatch"
        );
        self.assert_bytes(&source, b"copy bytes", "copy contract: source was modified");
        self.assert_bytes(&target, b"copy bytes", "copy contract: target bytes mismatch");
        self.assert_copy_conflicts(&source);
        if self.capable(FileSystemCapability::CreateDirectory) {
            let directory_source = self.path("copy-directory-source");
            self.fixture
                .file_system()
                .create_directory(&directory_source, CreateDirectoryOptions::default())
                .expect("copy contract: directory source creation failed");
            self.context.record_created(directory_source.clone());
            let directory_child = self.required_seed("copy-directory-source/child", b"directory copy", "copy");
            let directory_target = self.path("copy-directory-target");
            self.context.record_created(directory_target.clone());
            let target_child = self.path("copy-directory-target/child");
            self.context.record_created(target_child.clone());
            self.fixture
                .file_system()
                .copy(&directory_source, &directory_target, CopyOptions::tree())
                .expect("copy contract: directory copy failed");
            self.assert_bytes(
                &target_child,
                b"directory copy",
                "copy contract: directory child bytes mismatch",
            );
            self.context.record_created(directory_child);
        }
        if self.capable(FileSystemCapability::ServerSideCopy) {
            match self
                .fixture
                .copy_fast_path_case(CopyMethod::ServerSide)
                .expect("copy contract: fixture fast-path setup failed")
            {
                FixtureSupport::Supported(case) => {
                    self.context.record_created(case.source().clone());
                    self.context.record_created(case.target().clone());
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
                }
                FixtureSupport::Unsupported => {
                    panic!("copy contract: advertised native capability lacks an applicable fixture case")
                }
            }
        } else {
            let source = self.path("copy-server-side-unavailable-source");
            let target = self.path("copy-server-side-unavailable-target");
            let failure = self
                .fixture
                .file_system()
                .copy(
                    &source,
                    &target,
                    CopyOptions::default().with_server_side(ServerSidePreference::Require),
                )
                .expect_err("copy contract: unadvertised server-side copy succeeded");
            self.assert_requirement_error(
                failure.error(),
                FsOperation::Copy,
                FileSystemCapability::ServerSideCopy,
                "server-side-copy contract",
            );
        }
    }

    /// Checks destination conflict policies and copy statistics.
    pub fn assert_copy_conflicts(&mut self, source: &Path) {
        let target = self.required_seed("copy-conflict-target", b"existing", "copy-conflict");
        self.context.record_created(target.clone());
        let failure = self
            .fixture
            .file_system()
            .copy(source, &target, CopyOptions::file())
            .expect_err("copy contract: default conflict replaced target");
        self.assert_error(
            failure.error(),
            FsErrorKind::AlreadyExists,
            FsOperation::Copy,
            source,
            Some(&target),
        );
        self.assert_bytes(&target, b"existing", "copy contract: failed conflict changed target");

        let skipped = self
            .fixture
            .file_system()
            .copy(
                source,
                &target,
                CopyOptions::default()
                    .with_mode(CopyMode::File)
                    .with_conflict(CopyConflictPolicy::Skip),
            )
            .expect("copy contract: skip conflict failed");
        assert_eq!(skipped.stats().skipped, 1, "copy contract: skipped count mismatch");
        self.assert_bytes(&target, b"existing", "copy contract: skipped copy changed target");

        let overwritten = self
            .fixture
            .file_system()
            .copy(
                source,
                &target,
                CopyOptions::default()
                    .with_mode(CopyMode::File)
                    .with_conflict(CopyConflictPolicy::Overwrite),
            )
            .expect("copy contract: overwrite conflict failed");
        assert_eq!(
            overwritten.stats().overwritten,
            1,
            "copy contract: overwritten count mismatch"
        );
        self.assert_bytes(&target, b"copy bytes", "copy contract: overwrite bytes mismatch");
    }

    /// Checks required-durable copy publication when the provider advertises
    /// it.
    ///
    /// # Panics
    ///
    /// Panics when durable-copy preflight, publication, reported durability,
    /// or target content violates the advertised capability.
    pub fn assert_durable_copy(&mut self) {
        self.context.begin("durable_copy");
        let source = self.path("durable-copy-source");
        let target = self.path("durable-copy-target");
        let options = CopyOptions::default().with_durability(DurabilityRequirement::Required);
        if !self.capable(FileSystemCapability::DurableFileCopy) {
            let failure = self
                .fixture
                .file_system()
                .copy(&source, &target, options)
                .expect_err("durable-copy contract: unadvertised preflight succeeded");
            self.assert_requirement_error(
                failure.error(),
                FsOperation::Copy,
                FileSystemCapability::DurableFileCopy,
                "durable-copy contract",
            );
            return;
        }
        let source = self.required_seed("durable-copy-source", b"durable copy", "durable-copy");
        let target = self.path("durable-copy-target");
        self.context.record_created(source.clone());
        self.context.record_created(target.clone());
        let outcome = self
            .fixture
            .file_system()
            .copy(&source, &target, options)
            .expect("durable-copy contract: required-durable copy failed");
        assert!(
            outcome.durable(),
            "durable-copy contract: required operation reported non-durable publication"
        );
        self.assert_bytes(&target, b"durable copy", "durable-copy contract: target bytes mismatch");
    }
}
