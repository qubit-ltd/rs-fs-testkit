// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements copy and cancellation contracts.

use std::future::poll_fn;

use super::*;
use crate::CopyCancellationProbe;
use crate::FixtureError;

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
            let required = if !self.capable(FileSystemCapability::Read) {
                FileSystemCapability::Read
            } else if !self.capable(FileSystemCapability::Write) {
                FileSystemCapability::Write
            } else {
                FileSystemCapability::Copy
            };
            let error = match self
                .fixture
                .file_system()
                .begin_copy(source.clone(), target.clone(), CopyOptions::file())
            {
                Err(error) => error,
                Ok(mut operation) => operation
                    .execute()
                    .await
                    .expect_err("async copy contract: unavailable fallback succeeded"),
            };
            assert_error_with_source_or_target(
                error.error(),
                FsErrorKind::UnsupportedCapability,
                FsOperation::Copy,
                &source,
                &target,
                Some(self.context.properties().info().provider_id()),
                Some(required),
            );
            assert_eq!(
                error.error().required_capability(),
                Some(required),
                "async copy contract: fallback missing capability context"
            );
            self.record_copy_rejection();
            return;
        }
        if !self.capable(FileSystemCapability::Read) || !self.capable(FileSystemCapability::Write) {
            let source = self.path("async-copy-dependency-source");
            let target = self.path("async-copy-dependency-target");
            let mut operation = self
                .fixture
                .file_system()
                .begin_copy(source, target, CopyOptions::file())
                .expect("async copy contract: dependency preflight failed");
            let error = operation
                .execute()
                .await
                .expect_err("async copy contract: copy without read/write support succeeded");
            let required = if !self.capable(FileSystemCapability::Read) {
                FileSystemCapability::Read
            } else {
                FileSystemCapability::Write
            };
            assert_eq!(
                error.error().required_capability(),
                Some(required),
                "async copy contract: dependency error named the wrong capability"
            );
            self.record_copy_rejection();
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
        self.context.record_check(
            "copy/basic",
            Some(FileSystemCapability::Copy),
            ContractCheckOutcome::Passed,
        );
        self.assert_copy_conflicts(&source).await;
        self.assert_bytes(&target, b"copy bytes", "copy/basic: target bytes mismatch")
            .await;
        self.assert_atomic_copy(CopyMode::File).await;
        self.assert_atomic_copy(CopyMode::Tree).await;
        if self.capable(FileSystemCapability::ServerSideCopy) {
            let case = match self
                .fixture
                .copy_fast_path_case(CopyMethod::ServerSide)
                .await
                .expect("async copy contract: fast-path setup failed")
            {
                FixtureSupport::Supported(case) => case,
                FixtureSupport::Unsupported => {
                    self.context.record_check(
                        "copy/server-side",
                        Some(FileSystemCapability::ServerSideCopy),
                        ContractCheckOutcome::Unverified {
                            reason: "fixture cannot prepare an applicable server-side copy case".to_owned(),
                        },
                    );
                    self.assert_copy_cancellation_inner().await;
                    return;
                }
            };
            let snapshot = match self.fixture.read_file(case.source()).await {
                Ok(FixtureSupport::Supported(bytes)) => bytes,
                Ok(FixtureSupport::Unsupported) => {
                    self.context.record_check(
                        "copy/server-side",
                        Some(FileSystemCapability::ServerSideCopy),
                        ContractCheckOutcome::Unverified {
                            reason: "fixture cannot independently observe the server-side source".to_owned(),
                        },
                    );
                    self.assert_copy_cancellation_inner().await;
                    return;
                }
                Err(error) => {
                    panic!("async copy contract: server-side source snapshot failed: {error}")
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
            self.assert_bytes(
                case.source(),
                &snapshot,
                "async copy contract: server-side source changed",
            )
            .await;
            self.assert_bytes(
                case.target(),
                &snapshot,
                "async copy contract: server-side target mismatch",
            )
            .await;
            self.context.record_check(
                "copy/server-side",
                Some(FileSystemCapability::ServerSideCopy),
                ContractCheckOutcome::Passed,
            );
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
            self.context.record_check(
                "copy/server-side",
                Some(FileSystemCapability::ServerSideCopy),
                ContractCheckOutcome::RejectedAsExpected,
            );
        }
        self.assert_copy_cancellation_inner().await;
    }

    /// Records checks that cannot run when basic copy dependencies are absent.
    fn record_copy_rejection(&mut self) {
        self.context.record_check(
            "copy/basic",
            Some(FileSystemCapability::Copy),
            ContractCheckOutcome::RejectedAsExpected,
        );
        for (id, capability) in [
            ("copy/fallback-overwrite-rejected", FileSystemCapability::Copy),
            ("copy/server-side", FileSystemCapability::ServerSideCopy),
            ("copy/atomic-file", FileSystemCapability::AtomicFileCopy),
            ("copy/atomic-tree", FileSystemCapability::AtomicTreeCopy),
        ] {
            self.context.record_check(
                id,
                Some(capability),
                ContractCheckOutcome::NotApplicable {
                    reason: "basic copy dependencies are unavailable".to_owned(),
                },
            );
        }
    }

    /// Checks one required atomic copy variant with an exact file or tree
    /// fixture case.
    async fn assert_atomic_copy(&mut self, mode: CopyMode) {
        let (id, capability) = match mode {
            CopyMode::File => ("copy/atomic-file", FileSystemCapability::AtomicFileCopy),
            CopyMode::Tree => ("copy/atomic-tree", FileSystemCapability::AtomicTreeCopy),
            CopyMode::Auto => return,
        };
        if !self.capable(FileSystemCapability::Read)
            || !self.capable(FileSystemCapability::Write)
            || !self.capable(FileSystemCapability::Copy)
        {
            self.context.record_check(
                id,
                Some(capability),
                ContractCheckOutcome::NotApplicable {
                    reason: "atomic copy dependencies are unavailable".to_owned(),
                },
            );
            return;
        }
        if !self.capable(capability) {
            let source = self.path(&format!("{id}-rejected-source"));
            let target = self.path(&format!("{id}-rejected-target"));
            let failure = match self.fixture.file_system().begin_copy(
                source,
                target,
                CopyOptions::default()
                    .with_mode(mode)
                    .with_atomicity(AtomicityRequirement::Required),
            ) {
                Err(error) => error,
                Ok(mut operation) => operation
                    .execute()
                    .await
                    .expect_err("async copy contract: unsupported atomic copy succeeded"),
            };
            self.assert_requirement_error(failure.error(), FsOperation::Copy, capability, id);
            self.context
                .record_check(id, Some(capability), ContractCheckOutcome::RejectedAsExpected);
            return;
        }
        if mode == CopyMode::File {
            if matches!(
                self.fixture
                    .case_support(FixtureCase::Capability(capability))
                    .expect("async copy contract: atomic file fixture setup failed"),
                FixtureSupport::Unsupported
            ) {
                self.context.record_check(
                    id,
                    Some(capability),
                    ContractCheckOutcome::Unverified {
                        reason: "fixture cannot prepare an applicable atomic file case".to_owned(),
                    },
                );
                return;
            }
            let source = self.required_seed("async-atomic-copy-file-source", b"a", id).await;
            let target = self.path("async-atomic-copy-file-target");
            self.context.record_created(target.clone());
            let mut operation = self
                .fixture
                .file_system()
                .begin_copy(
                    source.clone(),
                    target.clone(),
                    CopyOptions::file().with_atomicity(AtomicityRequirement::Required),
                )
                .expect("async copy contract: atomic file preflight failed");
            let outcome = operation
                .execute()
                .await
                .expect("async copy contract: atomic file copy failed");
            assert_eq!(
                outcome.atomicity(),
                AchievedAtomicity::Atomic,
                "async copy contract: atomic file result was not atomic"
            );
            self.assert_bytes(&source, b"a", "async copy contract: atomic file source changed")
                .await;
            self.assert_bytes(&target, b"a", "async copy contract: atomic file target mismatch")
                .await;
            self.context
                .record_check(id, Some(capability), ContractCheckOutcome::Passed);
            return;
        }
        if matches!(
            self.fixture
                .case_support(FixtureCase::CopyTree)
                .expect("async copy contract: atomic tree fixture setup failed"),
            FixtureSupport::Unsupported
        ) {
            self.context.record_check(
                id,
                Some(capability),
                ContractCheckOutcome::Unverified {
                    reason: "fixture cannot prepare an applicable atomic tree case".to_owned(),
                },
            );
            return;
        }
        let (source, target, child, target_child) = self.prepare_tree_copy_case(id).await;
        let mut operation = self
            .fixture
            .file_system()
            .begin_copy(
                source,
                target,
                CopyOptions::tree().with_atomicity(AtomicityRequirement::Required),
            )
            .expect("async copy contract: atomic tree preflight failed");
        let outcome = operation
            .execute()
            .await
            .expect("async copy contract: atomic tree copy failed");
        assert_eq!(
            outcome.atomicity(),
            AchievedAtomicity::Atomic,
            "async copy contract: atomic tree result was not atomic"
        );
        self.assert_bytes(&child, b"b", "async copy contract: atomic tree source changed")
            .await;
        self.assert_bytes(&target_child, b"b", "copy/atomic-tree: atomic tree child mismatch")
            .await;
        self.context
            .record_check(id, Some(capability), ContractCheckOutcome::Passed);
    }

    /// Creates a small independently observed tree for one exact copy case.
    async fn prepare_tree_copy_case(&mut self, id: &'static str) -> (Path, Path, Path, Path) {
        let source = self.path("async-copy-tree-source");
        self.fixture
            .file_system()
            .create_directory(&source, CreateDirectoryOptions::default())
            .await
            .unwrap_or_else(|_| panic!("{id}: tree source creation failed"));
        self.context.record_created(source.clone());
        let child = self.required_seed("async-copy-tree-source/child", b"b", id).await;
        let target = self.path("async-copy-tree-target");
        let target_child = self.path("async-copy-tree-target/child");
        self.context.record_created(target.clone());
        self.context.record_created(target_child.clone());
        (source, target, child, target_child)
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
            self.record_cancellation_skips("basic copy capability is unavailable");
            return;
        }
        for stage in [
            AsyncCopyCancellationStage::NativeAttempt,
            AsyncCopyCancellationStage::Reader,
            AsyncCopyCancellationStage::Writer,
            AsyncCopyCancellationStage::Commit,
        ] {
            let relative = self.context.relative_name(&format!("async-copy-cancel-{stage:?}"));
            let prepared = self
                .fixture
                .prepare_copy_cancellation(stage, &relative)
                .await
                .expect("async copy cancellation contract: probe setup failed");
            let probe = match prepared {
                FixtureSupport::Supported(probe) => probe,
                FixtureSupport::Unsupported => {
                    self.record_legacy_cancellation_probe(stage);
                    continue;
                }
            };
            self.run_cancellation_probe(stage, probe).await;
        }
    }

    /// Runs one stage-aware cancellation probe with the caller's waker.
    async fn run_cancellation_probe(
        &mut self,
        stage: AsyncCopyCancellationStage,
        probe: Box<dyn CopyCancellationProbe>,
    ) {
        self.context.record_created(probe.case().source().clone());
        self.context.record_created(probe.case().target().clone());
        let case = probe.case().clone();
        let mut operation = self
            .fixture
            .file_system()
            .begin_copy(case.source().clone(), case.target().clone(), case.options().clone())
            .expect("async copy cancellation contract: preflight failed");
        let mut execution = Box::pin(operation.execute());
        let _disarm_on_unwind = DisarmOnDrop::new(probe.as_ref());
        let reached = poll_fn(|context| match execution.as_mut().poll(context) {
            Poll::Ready(Ok(_)) => Poll::Ready(Err(FixtureError::new(format!(
                "async copy cancellation contract: stage {stage:?} completed before acknowledgement"
            )))),
            Poll::Ready(Err(error)) => Poll::Ready(Err(FixtureError::new(format!(
                "async copy cancellation contract: stage {stage:?} failed before acknowledgement: {error}"
            )))),
            Poll::Pending => probe.poll_reached(context),
        })
        .await;
        let disarm = probe.disarm();
        drop(execution);
        disarm.expect("async copy cancellation contract: probe disarm failed");
        reached.expect("async copy cancellation contract: stage acknowledgement failed");
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
        self.context.record_check(
            cancellation_check_id(stage),
            Some(FileSystemCapability::Copy),
            ContractCheckOutcome::Passed,
        );
    }

    /// Records that an old request-only cancellation hook cannot acknowledge
    /// a provider-owned stage.
    fn record_legacy_cancellation_probe(&mut self, stage: AsyncCopyCancellationStage) {
        let support = self
            .fixture
            .copy_cancellation_case(stage)
            .expect("async copy cancellation contract: legacy probe setup failed");
        if let FixtureSupport::Supported(case) = support {
            self.context.record_created(case.source().clone());
            self.context.record_created(case.target().clone());
        }
        self.context.record_check(
            cancellation_check_id(stage),
            Some(FileSystemCapability::Copy),
            ContractCheckOutcome::SkippedOptional {
                reason: "stage acknowledgement unavailable from legacy copy_cancellation_case".to_owned(),
            },
        );
    }

    /// Records optional cancellation probes that cannot run for this fixture.
    fn record_cancellation_skips(&mut self, reason: &str) {
        for stage in [
            AsyncCopyCancellationStage::NativeAttempt,
            AsyncCopyCancellationStage::Reader,
            AsyncCopyCancellationStage::Writer,
            AsyncCopyCancellationStage::Commit,
        ] {
            self.context.record_check(
                cancellation_check_id(stage),
                Some(FileSystemCapability::Copy),
                ContractCheckOutcome::SkippedOptional {
                    reason: reason.to_owned(),
                },
            );
        }
    }

    /// Checks asynchronous destination conflict policies and statistics.
    pub async fn assert_copy_conflicts(&mut self, source: &Path) {
        match self
            .fixture
            .case_support(FixtureCase::CopyOverwrite)
            .expect("async copy contract: overwrite fixture case setup failed")
        {
            FixtureSupport::Supported(()) => {}
            FixtureSupport::Unsupported => {
                self.context.record_check(
                    "copy/fallback-overwrite-rejected",
                    Some(FileSystemCapability::Copy),
                    ContractCheckOutcome::Unverified {
                        reason: "fixture cannot prepare an applicable existing copy target".to_owned(),
                    },
                );
                return;
            }
        }
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
        self.context.record_check(
            "copy/fallback-overwrite-rejected",
            Some(FileSystemCapability::Copy),
            ContractCheckOutcome::Passed,
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
        self.assert_durable_variant(CopyMode::File).await;
        self.assert_durable_variant(CopyMode::Tree).await;
    }

    /// Checks one required durable file or tree copy without inferring the
    /// tree guarantee from the file guarantee.
    async fn assert_durable_variant(&mut self, mode: CopyMode) {
        let (id, capability) = match mode {
            CopyMode::File => ("copy/durable-file", FileSystemCapability::DurableFileCopy),
            CopyMode::Tree => ("copy/durable-tree", FileSystemCapability::DurableTreeCopy),
            CopyMode::Auto => return,
        };
        if !self.capable(FileSystemCapability::Read)
            || !self.capable(FileSystemCapability::Write)
            || !self.capable(FileSystemCapability::Copy)
        {
            self.context.record_check(
                id,
                Some(capability),
                ContractCheckOutcome::NotApplicable {
                    reason: "durable copy dependencies are unavailable".to_owned(),
                },
            );
            return;
        }
        if !self.capable(capability) {
            let source = self.path(&format!("{id}-rejected-source"));
            let target = self.path(&format!("{id}-rejected-target"));
            let options = if mode == CopyMode::File {
                CopyOptions::file()
            } else {
                CopyOptions::tree()
            }
            .with_durability(DurabilityRequirement::Required);
            let failure = match self.fixture.file_system().begin_copy(source, target, options) {
                Err(error) => error,
                Ok(mut operation) => operation
                    .execute()
                    .await
                    .expect_err("async copy contract: unsupported durable copy succeeded"),
            };
            self.assert_requirement_error(failure.error(), FsOperation::Copy, capability, id);
            self.context
                .record_check(id, Some(capability), ContractCheckOutcome::RejectedAsExpected);
            return;
        }
        if mode == CopyMode::File {
            if matches!(
                self.fixture
                    .case_support(FixtureCase::Capability(capability))
                    .expect("async copy contract: durable file fixture setup failed"),
                FixtureSupport::Unsupported
            ) {
                self.context.record_check(
                    id,
                    Some(capability),
                    ContractCheckOutcome::Unverified {
                        reason: "fixture cannot prepare an applicable durable file case".to_owned(),
                    },
                );
                return;
            }
            let source = self
                .required_seed("async-durable-copy-source", b"durable copy", id)
                .await;
            let target = self.path("async-durable-copy-target");
            self.context.record_created(target.clone());
            let mut operation = self
                .fixture
                .file_system()
                .begin_copy(
                    source.clone(),
                    target.clone(),
                    CopyOptions::file().with_durability(DurabilityRequirement::Required),
                )
                .expect("async copy contract: durable file preflight failed");
            let outcome = operation
                .execute()
                .await
                .expect("copy/durable-file: durable file copy failed");
            assert!(
                outcome.durable(),
                "async copy contract: durable file result was not durable"
            );
            self.assert_bytes(
                &target,
                b"durable copy",
                "async copy contract: durable file target mismatch",
            )
            .await;
            self.context
                .record_check(id, Some(capability), ContractCheckOutcome::Passed);
            return;
        }
        if matches!(
            self.fixture
                .case_support(FixtureCase::CopyTree)
                .expect("async copy contract: durable tree fixture setup failed"),
            FixtureSupport::Unsupported
        ) {
            self.context.record_check(
                id,
                Some(capability),
                ContractCheckOutcome::Unverified {
                    reason: "fixture cannot prepare an applicable durable tree case".to_owned(),
                },
            );
            return;
        }
        let (source, target, child, target_child) = self.prepare_tree_copy_case(id).await;
        let mut operation = self
            .fixture
            .file_system()
            .begin_copy(
                source,
                target,
                CopyOptions::tree().with_durability(DurabilityRequirement::Required),
            )
            .expect("async copy contract: durable tree preflight failed");
        let outcome = operation
            .execute()
            .await
            .expect("async copy contract: durable tree copy failed");
        assert!(
            outcome.durable(),
            "async copy contract: durable tree result was not durable"
        );
        self.assert_bytes(&child, b"b", "async copy contract: durable tree source changed")
            .await;
        self.assert_bytes(&target_child, b"b", "async copy contract: durable tree target mismatch")
            .await;
        self.context
            .record_check(id, Some(capability), ContractCheckOutcome::Passed);
    }
}

/// Returns the stable report identifier for one cancellation stage.
const fn cancellation_check_id(stage: AsyncCopyCancellationStage) -> &'static str {
    match stage {
        AsyncCopyCancellationStage::NativeAttempt => "async-copy/cancel-native-attempt",
        AsyncCopyCancellationStage::Reader => "async-copy/cancel-reader",
        AsyncCopyCancellationStage::Writer => "async-copy/cancel-writer",
        AsyncCopyCancellationStage::Commit => "async-copy/cancel-commit",
    }
}

/// Disarms a provider gate while an assertion is unwinding.
struct DisarmOnDrop<'a> {
    probe: &'a dyn CopyCancellationProbe,
}

impl<'a> DisarmOnDrop<'a> {
    /// Creates a drop guard for one stage-aware cancellation probe.
    fn new(probe: &'a dyn CopyCancellationProbe) -> Self {
        Self { probe }
    }
}

impl Drop for DisarmOnDrop<'_> {
    /// Releases the provider gate without starting asynchronous cleanup.
    fn drop(&mut self) {
        let _ = self.probe.disarm();
    }
}
