// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements copy and cancellation contracts.

use super::*;

impl<'a> AsyncFileSystemContractSuite<'a> {
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
            let target = self.path("async-copy-unavailable-target");
            let mut operation = self
                .fixture
                .file_system()
                .begin_copy(source.clone(), target.clone(), Default::default())
                .expect("async copy contract: fallback selection failed");
            let error = operation
                .execute()
                .await
                .expect_err("async copy contract: unavailable fallback succeeded");
            assert_error_with_target(
                error.error(),
                FsErrorKind::UnsupportedCapability,
                FsOperation::Copy,
                Some(&source),
                Some(&target),
                Some(self.context.properties().info().provider_id()),
                Some(FileSystemCapability::Read),
            );
            assert_eq!(
                error.error().required_capability(),
                Some(FileSystemCapability::Read),
                "async copy contract: fallback missing read-capability context"
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
        let outcome = operation.execute().await.expect("async copy contract: copy failed");
        match outcome.method() {
            CopyMethod::Streamed => assert!(
                outcome.used_fallback(),
                "async copy contract: streamed copy was not reported as fallback"
            ),
            CopyMethod::Native | CopyMethod::Clone | CopyMethod::ServerSide | CopyMethod::Mixed => {
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
        self.assert_bytes(&source, b"copy bytes", "async copy contract: source was modified")
            .await;
        self.assert_copy_conflicts(&source).await;
        self.assert_bytes(&target, b"copy bytes", "async copy contract: target bytes mismatch")
            .await;
        if self.capable(FileSystemCapability::CreateDirectory) {
            let directory_source = self.path("async-copy-directory-source");
            self.fixture
                .file_system()
                .create_directory(&directory_source, CreateDirectoryOptions::default())
                .await
                .expect("async copy contract: directory source creation failed");
            self.context.record_created(directory_source.clone());
            let directory_child = self
                .required_seed("async-copy-directory-source/child", b"directory copy", "copy")
                .await;
            let directory_target = self.path("async-copy-directory-target");
            self.context.record_created(directory_target.clone());
            let target_child = self.path("async-copy-directory-target/child");
            self.context.record_created(target_child.clone());
            let mut operation = self
                .fixture
                .file_system()
                .begin_copy(directory_source, directory_target.clone(), CopyOptions::tree())
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
                FixtureSupport::Unsupported => {
                    panic!("async copy contract: advertised server-side capability lacks an applicable fixture case")
                }
            };
            self.context.record_created(case.source().clone());
            self.context.record_created(case.target().clone());
            let mut operation = self
                .fixture
                .file_system()
                .begin_copy(case.source().clone(), case.target().clone(), case.options().clone())
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
                CopyOptions::default().with_server_side(ServerSidePreference::Require),
            ) {
                Err(error) => error,
                Ok(_) => panic!("async copy contract: unadvertised server-side copy succeeded"),
            };
            self.assert_requirement_error(
                error.error(),
                FsOperation::Copy,
                FileSystemCapability::ServerSideCopy,
                "server-side-copy contract",
            );
        }
        self.assert_copy_cancellation_inner().await;
    }

    /// Checks asynchronous copy cancellation and recovery-state semantics.
    ///
    /// The fixture may opt out of individual probes when it cannot control a
    /// provider-owned pending stage. Advertised copy support is still required
    /// for this phase; unsupported copy returns without running probes.
    ///
    /// # Panics
    ///
    /// Panics when a supported cancellation probe does not suspend, does not
    /// transition to an indeterminate state, or loses a recoverable writer.
    pub async fn assert_copy_cancellation(&mut self) {
        self.context.begin("copy-cancellation");
        self.assert_copy_cancellation_inner().await;
    }

    /// Runs the cancellation probes without changing the current phase name.
    pub async fn assert_copy_cancellation_inner(&mut self) {
        if !self.capable(FileSystemCapability::Copy) {
            return;
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
                .expect("async copy cancellation contract: fixture setup failed");
            let case = match case {
                FixtureSupport::Supported(case) => case,
                FixtureSupport::Unsupported => continue,
            };
            self.context.record_created(case.source().clone());
            self.context.record_created(case.target().clone());
            let mut operation = self
                .fixture
                .file_system()
                .begin_copy(case.source().clone(), case.target().clone(), case.options().clone())
                .expect("async copy cancellation contract: preflight failed");
            let mut execution = Box::pin(operation.execute());
            let waker = Waker::noop();
            let mut task = Context::from_waker(waker);
            assert!(
                matches!(execution.as_mut().poll(&mut task), Poll::Pending),
                "async copy cancellation contract: fixture stage {stage:?} did not pend"
            );
            drop(execution);
            assert_eq!(
                operation.state(),
                AsyncCopyOperationState::Failed(CopyFailureState::Indeterminate),
                "async copy cancellation contract: cancellation did not become indeterminate"
            );
            if matches!(
                stage,
                AsyncCopyCancellationStage::Writer | AsyncCopyCancellationStage::Commit
            ) {
                assert!(
                    operation.take_recovery_writer().is_some(),
                    "async copy cancellation contract: recovery writer was lost"
                );
            }
        }
    }

    /// Checks asynchronous destination conflict policies and statistics.
    pub async fn assert_copy_conflicts(&mut self, source: &Path) {
        let target = self
            .required_seed("async-copy-conflict-target", b"existing", "copy-conflict")
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
        assert_error_with_source_or_target(
            failure.error(),
            FsErrorKind::AlreadyExists,
            FsOperation::Copy,
            source,
            &target,
            Some(self.context.properties().info().provider_id()),
            None,
        );
        self.assert_bytes(&target, b"existing", "copy contract: failed conflict changed target")
            .await;

        let mut operation = self
            .fixture
            .file_system()
            .begin_copy(
                source.clone(),
                target.clone(),
                CopyOptions::default()
                    .with_mode(CopyMode::File)
                    .with_conflict(CopyConflictPolicy::Skip),
            )
            .expect("copy contract: skip preflight failed");
        let skipped = operation.execute().await.expect("copy contract: skip conflict failed");
        assert_eq!(skipped.stats().skipped, 1);
        self.assert_bytes(&target, b"existing", "copy contract: skipped copy changed target")
            .await;

        let mut operation = self
            .fixture
            .file_system()
            .begin_copy(
                source.clone(),
                target.clone(),
                CopyOptions::default()
                    .with_mode(CopyMode::File)
                    .with_conflict(CopyConflictPolicy::Overwrite),
            )
            .expect("copy contract: overwrite preflight failed");
        let overwritten = operation
            .execute()
            .await
            .expect("copy contract: overwrite conflict failed");
        assert_eq!(overwritten.stats().overwritten, 1);
        self.assert_bytes(&target, b"copy bytes", "copy contract: overwrite bytes mismatch")
            .await;
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
        let options = CopyOptions::default().with_durability(DurabilityRequirement::Required);
        if !self.capable(FileSystemCapability::DurableFileCopy) {
            let error = match self.fixture.file_system().begin_copy(source, target, options) {
                Err(error) => error,
                Ok(_) => panic!("durable-copy contract: unadvertised preflight succeeded"),
            };
            self.assert_requirement_error(
                error.error(),
                FsOperation::Copy,
                FileSystemCapability::DurableFileCopy,
                "durable-copy contract",
            );
            return;
        }
        let source = self
            .required_seed("async-durable-copy-source", b"durable copy", "durable-copy")
            .await;
        let mut operation = self
            .fixture
            .file_system()
            .begin_copy(source.clone(), target.clone(), options)
            .expect("durable-copy contract: preflight failed");
        self.context.record_created(target.clone());
        let outcome = operation.execute().await.expect("durable-copy contract: copy failed");
        assert!(outcome.durable(), "durable-copy contract: non-durable outcome");
        self.assert_bytes(&target, b"durable copy", "durable-copy contract: target bytes mismatch")
            .await;
    }
}
