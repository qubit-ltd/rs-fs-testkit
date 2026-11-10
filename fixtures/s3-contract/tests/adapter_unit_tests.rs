// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use qubit_fs::Path;
use qubit_fs::error::FsErrorKind;
use qubit_fs_s3_contract::S3ContractConfig;
use qubit_fs_s3_contract::map;
use qubit_fs_s3_contract::open_in_memory;
use qubit_fs_s3_contract::validate_key;
use qubit_io::AsyncOutput;

#[test]
fn test_object_keys_are_preserved_and_prefixed() {
    let config = S3ContractConfig {
        endpoint: "https://example.invalid".into(),
        bucket: "bucket".into(),
        region: "us-east-1".into(),
        access_key_id: "secret-id".into(),
        secret_access_key: "secret-value".into(),
        prefix: "run-1".into(),
        allow_http: false,
    };
    let path = Path::parse_literal("a b/%E4/文件").unwrap();
    assert_eq!(map(&config, &path).unwrap(), "run-1/a b/%E4/文件");
}

#[test]
fn test_dot_segments_are_rejected() {
    assert!(validate_key("a/../b").is_err());
    assert!(validate_key("a/./b").is_err());
    assert!(validate_key("a/%2e%2e/b").is_ok());
}

#[test]
fn test_invalid_prefix_is_rejected_without_normalizing_object_keys() {
    let config = S3ContractConfig {
        prefix: "run/../bad".into(),
        endpoint: "https://example.invalid".into(),
        bucket: "bucket".into(),
        region: "us-east-1".into(),
        access_key_id: "id".into(),
        secret_access_key: "secret".into(),
        allow_http: false,
    };
    let path = Path::parse_literal("a/%2e%2e/b").unwrap();
    assert!(map(&config, &path).is_err());
    let config = S3ContractConfig {
        prefix: "run".into(),
        ..config
    };
    assert_eq!(map(&config, &path).unwrap(), "run/a/%2e%2e/b");
}

#[test]
fn test_configuration_debug_does_not_expose_credentials() {
    let config = S3ContractConfig {
        endpoint: "https://example.invalid".into(),
        bucket: "bucket".into(),
        region: "us-east-1".into(),
        access_key_id: "secret-id".into(),
        secret_access_key: "secret-value".into(),
        prefix: "run-1".into(),
        allow_http: false,
    };
    let debug = format!("{config:?}");
    assert!(!debug.contains("secret-id"));
    assert!(!debug.contains("secret-value"));
}

mod common;

/// A fixture decorator that keeps probe acknowledgement and optional teardown pending.
struct ControlledDeadlineFixture {
    inner: common::Fixture,
    teardown_pending: bool,
}

#[derive(Debug, Eq, PartialEq)]
enum DeadlineOutcome {
    Completed,
    RunTimedOut {
        provider_stage_reached: bool,
        interrupted: bool,
        cleanup_timed_out: bool,
        cleanup_completed: bool,
    },
}

struct NeverAcknowledgeProbe {
    inner: Box<dyn qubit_fs_testkit::WriteCancellationProbe>,
}

impl qubit_fs_testkit::WriteCancellationProbe for NeverAcknowledgeProbe {
    fn case(&self) -> &qubit_fs_testkit::AsyncWriteFixtureCase {
        self.inner.case()
    }

    fn poll_reached(
        &self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<qubit_fs_testkit::FixtureResult<()>> {
        match self.inner.poll_reached(cx) {
            std::task::Poll::Ready(Ok(())) => std::task::Poll::Pending,
            result => result,
        }
    }

    fn accepted_bytes(&self) -> qubit_fs_testkit::FixtureResult<u64> {
        self.inner.accepted_bytes()
    }

    fn observe_target(&self) -> qubit_fs_testkit::FixtureFuture<'_, Option<Vec<u8>>> {
        self.inner.observe_target()
    }

    fn disarm(&self) -> qubit_fs_testkit::FixtureResult<()> {
        self.inner.disarm()
    }
}

impl qubit_fs_testkit::AsyncFileSystemFixture for ControlledDeadlineFixture {
    fn file_system(&self) -> &qubit_fs::AsyncFileSystem {
        &self.inner.filesystem
    }

    fn path(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<qubit_fs::Path> {
        qubit_fs_testkit::AsyncFileSystemFixture::path(&self.inner, relative)
    }

    fn prepare_write_cancellation<'a>(
        &'a self,
        stage: qubit_fs_testkit::AsyncWriteCancellationStage,
        relative: &'a str,
    ) -> qubit_fs_testkit::FixtureFuture<'a, qubit_fs_testkit::FixtureSupport<Box<dyn qubit_fs_testkit::WriteCancellationProbe>>> {
        let prepared = qubit_fs_testkit::AsyncFileSystemFixture::prepare_write_cancellation(
            &self.inner,
            stage,
            relative,
        );
        Box::pin(async move {
            match prepared.await? {
                qubit_fs_testkit::FixtureSupport::Supported(probe) => {
                    let probe: Box<dyn qubit_fs_testkit::WriteCancellationProbe> =
                        Box::new(NeverAcknowledgeProbe { inner: probe });
                    Ok(qubit_fs_testkit::FixtureSupport::Supported(probe))
                }
                qubit_fs_testkit::FixtureSupport::Unsupported => {
                    Ok(qubit_fs_testkit::FixtureSupport::Unsupported)
                }
            }
        })
    }

    fn teardown(&self) -> qubit_fs_testkit::FixtureFuture<'_, ()> {
        if self.teardown_pending {
            Box::pin(std::future::pending::<qubit_fs_testkit::FixtureResult<()>>())
        } else {
            qubit_fs_testkit::AsyncFileSystemFixture::teardown(&self.inner)
        }
    }
}

/// Executes a real suite check under separate run and cleanup budgets.
///
/// When `control` is provided, the run is held after the real provider reaches
/// its write-open gate, so timeout assertions inspect retained suite evidence.
async fn run_suite_check_with_budgets(
    suite: &mut qubit_fs_testkit::AsyncFileSystemContractSuite<'_>,
    id: qubit_fs_testkit::ContractCheckId,
    run_budget: std::time::Duration,
    cleanup_budget: std::time::Duration,
    control: Option<&qubit_fs_s3_contract::TestControl>,
) -> DeadlineOutcome {
    use std::future::poll_fn;

    let mut provider_stage_reached = false;
    let run = tokio::time::timeout(run_budget, async {
        if let Some(control) = control {
            tokio::select! {
                _ = suite.run_check(id) => {
                    panic!("write-open cancellation check completed before its deadline");
                }
                _ = poll_fn(|cx| control.poll_reached(cx)) => {
                    provider_stage_reached = true;
                    std::future::pending::<()>().await;
                }
            }
        } else {
            suite.run_check(id).await;
        }
    })
    .await;
    if run.is_err() {
        let interrupted = suite.run().was_interrupted();
        let cleanup = tokio::time::timeout(cleanup_budget, suite.finish()).await;
        return DeadlineOutcome::RunTimedOut {
            provider_stage_reached,
            interrupted,
            cleanup_timed_out: cleanup.is_err(),
            cleanup_completed: suite.run().cleanup().completed(),
        };
    }
    suite.run().assert_satisfied_with(&[id]);
    DeadlineOutcome::Completed
}

/// Runs one check with caller-owned run and cleanup deadlines.
async fn run_check_with_deadlines(
    fixture: &common::Fixture,
    id: qubit_fs_testkit::ContractCheckId,
) {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    let mut suite = AsyncFileSystemContractSuite::new(fixture);
    match run_suite_check_with_budgets(
        &mut suite,
        id,
        std::time::Duration::from_secs(30),
        std::time::Duration::from_secs(30),
        None,
    )
    .await
    {
        DeadlineOutcome::Completed => {}
        DeadlineOutcome::RunTimedOut {
            provider_stage_reached,
            interrupted,
            cleanup_timed_out,
            cleanup_completed,
        } => panic!(
            "contract {id}: run deadline reached; provider stage reached={provider_stage_reached}; interrupted={interrupted}; cleanup timed out={cleanup_timed_out}; cleanup completed={cleanup_completed}"
        ),
    }
}

#[tokio::test]
async fn test_documented_bounded_run_helper_compiles_and_runs() {
    run_check_with_deadlines(
        &common::Fixture::memory("bounded-run"),
        qubit_fs_testkit::ContractCheckId::StatBasic,
    )
    .await;
}

/// A provider-stage timeout retains interruption evidence and cleans successfully.
#[tokio::test(start_paused = true)]
async fn test_real_suite_run_timeout_retains_evidence_and_cleanup_succeeds() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;
    use std::time::Duration;

    let fixture = ControlledDeadlineFixture {
        inner: common::Fixture::memory("run-timeout-cleanup-ok"),
        teardown_pending: false,
    };
    let control = fixture.inner.control.clone();
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);

    assert_eq!(
        run_suite_check_with_budgets(
            &mut suite,
            qubit_fs_testkit::ContractCheckId::WriteCancelOpen,
            Duration::from_secs(1),
            Duration::from_secs(5),
            Some(&control),
        )
        .await,
        DeadlineOutcome::RunTimedOut {
            provider_stage_reached: true,
            interrupted: true,
            cleanup_timed_out: false,
            cleanup_completed: true,
        }
    );
    assert!(suite.run().was_interrupted());
    assert!(!suite.run().requirements_satisfied());
    assert_ne!(
        suite.run().report().checks()[0].outcome(),
        &qubit_fs_testkit::ContractCheckOutcome::Passed
    );

    assert!(suite.run().cleanup().completed());
    assert!(!suite.run().requirements_satisfied(), "cleanup cannot turn a run timeout into a pass");
}

/// A provider-stage run timeout remains interrupted when cleanup also expires.
#[tokio::test(start_paused = true)]
async fn test_real_suite_run_and_cleanup_timeouts_retain_incomplete_cleanup() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;
    use std::time::Duration;

    let fixture = ControlledDeadlineFixture {
        inner: common::Fixture::memory("run-and-cleanup-timeout"),
        teardown_pending: true,
    };
    let control = fixture.inner.control.clone();
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);

    assert_eq!(
        run_suite_check_with_budgets(
            &mut suite,
            qubit_fs_testkit::ContractCheckId::WriteCancelOpen,
            Duration::from_secs(1),
            Duration::from_secs(5),
            Some(&control),
        )
        .await,
        DeadlineOutcome::RunTimedOut {
            provider_stage_reached: true,
            interrupted: true,
            cleanup_timed_out: true,
            cleanup_completed: false,
        }
    );
    assert!(suite.run().was_interrupted());
    assert!(!suite.run().requirements_satisfied());
    assert_ne!(
        suite.run().report().checks()[0].outcome(),
        &qubit_fs_testkit::ContractCheckOutcome::Passed
    );

    assert!(!suite.run().cleanup().completed());
    assert_eq!(suite.run().cleanup().attempts(), 1);
    assert!(!suite.run().requirements_satisfied());
}

/// Each supported contract runs against a fresh independent SDK fixture.
#[tokio::test]
async fn test_in_memory_adapter_runs_the_supported_contract_matrix() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;
    use qubit_fs_testkit::ContractCheckId;
    use qubit_fs_testkit::FileSystemContract;
    for contract in [
        FileSystemContract::Properties,
        FileSystemContract::Stat,
        FileSystemContract::Read,
        FileSystemContract::List,
    ] {
        let fixture = common::Fixture::memory("contract-run");
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        suite.run_contract(contract).await.assert_satisfied();
    }
    for id in [
        ContractCheckId::WriteBasic,
        ContractCheckId::WriteCreateConflict,
        ContractCheckId::WriteAbort,
        ContractCheckId::WriteLimit,
        ContractCheckId::WriteOwningOperation,
        ContractCheckId::WriteIfAbsent,
        ContractCheckId::WriteIfMatch,
        ContractCheckId::WriteCancelOpen,
        ContractCheckId::WriteCancelWrite,
        ContractCheckId::WriteCancelFlush,
        ContractCheckId::WriteCancelCommit,
    ] {
        let fixture = common::Fixture::memory("write-contract-run");
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        suite.run_check(id).await.assert_satisfied();
    }
}

#[tokio::test]
async fn test_create_only_collision_is_not_published_and_keeps_payload() {
    use qubit_fs::write::WriteDisposition;
    use qubit_fs::write::WriteFailureState;
    use qubit_fs::write::WriteOptions;

    let filesystem = open_in_memory("writer-recovery").unwrap();
    let path = Path::parse_literal("same-key").unwrap();
    let options = WriteOptions::default().with_disposition(WriteDisposition::CreateNew);
    let mut first = filesystem.open_writer(&path, options.clone()).await.unwrap();
    first.write_fully_async(b"existing").await.unwrap();
    first.commit_async().await.unwrap();

    let mut second = filesystem.open_writer(&path, options).await.unwrap();
    second.write_fully_async(b"retry-payload").await.unwrap();
    let failure = second.commit_async().await.unwrap_err();
    assert_eq!(failure.state(), WriteFailureState::NotPublished);
    assert_eq!(
        filesystem.read_all(&path, Default::default(), 64).await.unwrap(),
        b"existing"
    );
    let retry = second.commit_async().await.unwrap_err();
    assert_eq!(retry.state(), WriteFailureState::NotPublished);
}

#[tokio::test]
async fn test_empty_object_reports_zero_length() {
    use qubit_fs::write::WriteDisposition;
    use qubit_fs::write::WriteOptions;

    let filesystem = open_in_memory("empty-object").unwrap();
    let path = Path::parse_literal("empty").unwrap();
    let options = WriteOptions::default().with_disposition(WriteDisposition::CreateNew);
    let mut writer = filesystem.open_writer(&path, options).await.unwrap();
    writer.commit_async().await.unwrap();
    assert_eq!(filesystem.stat(&path).await.unwrap().len(), Some(0));
}

#[tokio::test]
async fn test_in_memory_listing_preserves_literal_object_prefixes() {
    use qubit_fs::directory::ListOptions;
    use qubit_fs::write::WriteDisposition;
    use qubit_fs::write::WriteOptions;

    let filesystem = open_in_memory("list-contract").unwrap();
    for key in ["folder/a", "folder/ab", "other"] {
        let path = Path::parse_literal(key).unwrap();
        let options = WriteOptions::default().with_disposition(WriteDisposition::CreateNew);
        let mut operation = filesystem.begin_write_all(path, b"x".to_vec(), options).unwrap();
        operation.execute().await.unwrap();
    }
    let root = Path::parse_literal("folder").unwrap();
    let mut stream = filesystem
        .list(&qubit_fs::directory::ListScope::Path(root), ListOptions::object_keys())
        .await
        .unwrap();
    let mut names = Vec::new();
    while let Some(entry) = stream.next_entry_async().await.unwrap() {
        names.push(entry.path.as_str().to_owned());
    }
    names.sort();
    assert_eq!(names, ["folder/a", "folder/ab"]);
}

#[tokio::test]
async fn test_unsupported_writer_options_fail_before_publication() {
    use qubit_fs::write::WriteDisposition;
    use qubit_fs::write::WriteOptions;
    let filesystem = open_in_memory("writer-options").unwrap();
    let path = Path::parse_literal("options").unwrap();
    let options = WriteOptions::default()
        .with_disposition(WriteDisposition::CreateNew)
        .with_content_type(Some("text/plain".into()));
    let error = filesystem.open_writer(&path, options).await.unwrap_err();
    assert_eq!(error.error().kind(), FsErrorKind::RequirementNotMet);
    assert!(error.recovery().is_none());
}

/// The same recovery matrix used by opt-in real-service tests runs locally.
#[tokio::test]
async fn test_shared_recovery_matrix_runs_in_memory() {
    use futures_util::FutureExt;
    use qubit_fs_testkit::AsyncFileSystemFixture;
    let fixture = common::Fixture::memory("shared-recovery");
    let result = std::panic::AssertUnwindSafe(common::recovery_matrix::verify(&fixture))
        .catch_unwind()
        .await;
    let cleanup = fixture.teardown().await;
    cleanup.expect("exact-key cleanup");
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

#[tokio::test]
async fn test_expected_conditional_failures_abort_the_retained_session() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;
    use qubit_fs_testkit::ContractCheckId;
    for id in [ContractCheckId::WriteIfAbsent, ContractCheckId::WriteIfMatch] {
        let fixture = common::Fixture::memory("expected-cleanup");
        AsyncFileSystemContractSuite::new(&fixture)
            .run_check(id)
            .await
            .assert_satisfied();
        assert_eq!(
            fixture.control.abort_calls(),
            1,
            "expected rejection must release its session"
        );
    }
}

#[tokio::test]
async fn test_conditional_cleanup_failure_retains_original_snapshot_and_session() {
    use qubit_fs::write::AsyncWriterRecovery;
    use qubit_fs::write::WriteAbortOutcome;
    use qubit_fs::write::WriteFailureState;
    use qubit_fs_testkit::AsyncFileSystemContractSuite;
    use qubit_fs_testkit::ContractAsyncWriteFailure;
    use qubit_fs_testkit::ContractCheckId;
    use qubit_fs_testkit::ContractSource;
    use qubit_fs_testkit::ContractWriterFailure;
    let fixture = common::Fixture::memory("expected-cleanup");
    fixture.control.fail_next_abort();
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteIfAbsent).await;
    assert!(
        !run.requirements_satisfied(),
        "cleanup failure cannot count as a passed check"
    );
    let source = std::error::Error::source(&run.failures()[0])
        .expect("source")
        .downcast_ref::<ContractSource>()
        .expect("owned source");
    let mut retained = source
        .take()
        .expect("owned failure")
        .downcast::<ContractWriterFailure<ContractAsyncWriteFailure>>()
        .expect("both failures and operation");
    assert_eq!(retained.writer().error().kind(), FsErrorKind::PreconditionFailed);
    let state = retained.writer().failure().state();
    let bytes = retained.writer().failure().written_bytes();
    assert_eq!(state, WriteFailureState::NotPublished);
    let operation = retained.writer_mut().operation_mut().expect("retained operation");
    let Some(AsyncWriterRecovery::Opened(writer)) = operation.recovery() else {
        panic!("opened session")
    };
    assert_eq!(
        writer.abort_async().await.expect("explicit retry"),
        WriteAbortOutcome::NotPublished
    );
    assert_eq!(retained.writer().failure().state(), state);
    assert_eq!(retained.writer().failure().written_bytes(), bytes);
    assert_eq!(fixture.control.abort_calls(), 2);
}

#[tokio::test]
async fn test_cancelling_expected_rejection_cleanup_preserves_session_in_run() {
    use std::future::Future;

    use qubit_fs::write::AsyncWriterRecovery;
    use qubit_fs::write::WriteAbortOutcome;
    use qubit_fs_s3_contract::TestStage;
    use qubit_fs_testkit::AsyncFileSystemContractSuite;
    use qubit_fs_testkit::ContractAsyncWriteFailure;
    use qubit_fs_testkit::ContractCheckId;
    use qubit_fs_testkit::ContractSource;
    let fixture = common::Fixture::memory("cancel-expected-cleanup");
    fixture.control.arm(TestStage::Abort);
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let mut checking = Box::pin(suite.run_check(ContractCheckId::WriteIfAbsent));
    std::future::poll_fn(|cx| {
        assert!(checking.as_mut().poll(cx).is_pending());
        fixture.control.poll_reached(cx)
    })
    .await;
    drop(checking);
    fixture.control.release();
    let run = suite.run();
    assert!(run.was_interrupted());
    assert!(
        !run.failures().is_empty(),
        "cancelled cleanup must preserve the original rejection"
    );
    let source = std::error::Error::source(&run.failures()[0])
        .expect("source")
        .downcast_ref::<ContractSource>()
        .expect("owned source");
    let mut failure = source
        .take()
        .expect("original failure")
        .downcast::<ContractAsyncWriteFailure>()
        .expect("operation ownership");
    let bytes = failure.failure().written_bytes();
    let Some(AsyncWriterRecovery::Opened(writer)) = failure.operation_mut().expect("operation").recovery() else {
        panic!("retained writer")
    };
    assert_eq!(
        writer.abort_async().await.expect("explicit cleanup retry"),
        WriteAbortOutcome::NotPublished
    );
    assert_eq!(failure.failure().written_bytes(), bytes);
}
