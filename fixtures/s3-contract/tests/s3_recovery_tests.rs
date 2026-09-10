//! SDK-boundary recovery and atomic conditional-write evidence.
use std::sync::Arc;

use bytes::Bytes;
use object_store::ObjectStore;
use object_store::ObjectStoreExt;
use object_store::path::Path as ObjectPath;
use qubit_fs::Path;
use qubit_fs::metadata::ResourceVersion;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteFailureState;
use qubit_fs::write::WriteOptions;
use qubit_fs::write::WritePrecondition;
use qubit_fs_s3_contract::S3ContractConfig;
use qubit_fs_s3_contract::open_with_store;

/// All observations use the independent SDK store, not the filesystem facade.
fn setup() -> (qubit_fs::AsyncFileSystem, Arc<dyn ObjectStore>) {
    let store: Arc<dyn ObjectStore> = Arc::new(object_store::memory::InMemory::new());
    let config = S3ContractConfig {
        endpoint: "memory://".into(),
        bucket: "contract".into(),
        region: "local".into(),
        access_key_id: "test".into(),
        secret_access_key: "test".into(),
        prefix: "recovery".into(),
        allow_http: false,
    };
    (open_with_store(config, Arc::clone(&store)).expect("facade"), store)
}

/// Two writers using one observed ETag cannot both replace the object.
#[tokio::test]
async fn conditional_update_rejects_stale_etag_without_overwriting() {
    let (fs, store) = setup();
    let key = ObjectPath::from("recovery/conditional");
    let seed = store.put(&key, Bytes::from_static(b"old").into()).await.expect("seed");
    let version = ResourceVersion::new(seed.e_tag.expect("ETag"));
    let path = Path::parse_literal("conditional").expect("path");
    let options = WriteOptions::default()
        .with_disposition(WriteDisposition::CreateOrReplace)
        .with_precondition(WritePrecondition::IfMatch(version));
    let mut first = fs
        .begin_write_all(path.clone(), b"new".to_vec(), options.clone())
        .expect("first");
    let mut stale = fs.begin_write_all(path, b"stale".to_vec(), options).expect("stale");
    first.execute().await.expect("matching ETag");
    let failure = stale.execute().await.expect_err("stale ETag");
    assert_eq!(failure.state(), WriteFailureState::NotPublished);
    assert_eq!(
        store.get(&key).await.expect("observe").bytes().await.expect("bytes"),
        b"new".as_slice()
    );
}

/// Creates independent SDK observation and explicit adapter gates.
fn controlled() -> (
    qubit_fs::AsyncFileSystem,
    Arc<dyn ObjectStore>,
    qubit_fs_s3_contract::TestControl,
) {
    let store: Arc<dyn ObjectStore> = Arc::new(object_store::memory::InMemory::new());
    let control = qubit_fs_s3_contract::TestControl::default();
    let config = S3ContractConfig {
        endpoint: "memory://".into(),
        bucket: "contract".into(),
        region: "local".into(),
        access_key_id: "test".into(),
        secret_access_key: "test".into(),
        prefix: "recovery".into(),
        allow_http: false,
    };
    let fs = qubit_fs_s3_contract::open_with_control(config, Arc::clone(&store), control.clone()).expect("facade");
    (fs, store, control)
}

/// Advances an operation until the selected adapter stage explicitly
/// acknowledges it.
async fn reach<F: std::future::Future + ?Sized>(
    mut future: std::pin::Pin<&mut F>,
    control: &qubit_fs_s3_contract::TestControl,
) {
    std::future::poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending(), "operation completed before gate");
        control.poll_reached(cx)
    })
    .await;
}

/// Cancellation at each SDK boundary freezes facts and never dispatches an
/// automatic retry.
#[tokio::test]
async fn stage_cancellation_preserves_operation_and_independent_publication_evidence() {
    use qubit_fs::write::AsyncWriteAllOperationState;
    use qubit_fs::write::AsyncWriterRecovery;
    use qubit_fs::write::WriteAbortOutcome;
    use qubit_fs_s3_contract::TestStage;
    for stage in [
        TestStage::Open,
        TestStage::Write,
        TestStage::Flush,
        TestStage::BeforePut,
        TestStage::AfterPutBeforeResult,
    ] {
        let (fs, store, control) = controlled();
        control.arm(stage);
        let path = Path::parse_literal("cancelled").expect("path");
        let key = ObjectPath::from("recovery/cancelled");
        let mut operation = fs
            .begin_write_all(
                path,
                b"payload".to_vec(),
                WriteOptions::default().with_disposition(WriteDisposition::CreateNew),
            )
            .expect("operation");
        drop(operation.execute());
        assert_eq!(operation.state(), AsyncWriteAllOperationState::Ready);
        assert_eq!(control.put_calls(), 0);
        let mut execute = Box::pin(operation.execute());
        reach(execute.as_mut(), &control).await;
        if stage == TestStage::AfterPutBeforeResult {
            assert_eq!(
                store
                    .get(&key)
                    .await
                    .expect("published before result delivery")
                    .bytes()
                    .await
                    .expect("bytes"),
                b"payload".as_slice()
            );
        } else {
            assert!(matches!(
                store.head(&key).await,
                Err(object_store::Error::NotFound { .. })
            ));
        }
        drop(execute);
        control.release();
        assert_eq!(
            operation.state(),
            AsyncWriteAllOperationState::Failed(WriteFailureState::Indeterminate)
        );
        let confirmed = if matches!(stage, TestStage::Open | TestStage::Write) {
            0
        } else {
            7
        };
        assert_eq!(operation.written_bytes(), confirmed);
        assert_eq!(operation.has_recovery(), stage != TestStage::Open);
        if let Some(recovery) = operation.take_recovery() {
            let AsyncWriterRecovery::Opened(mut writer) = recovery else {
                panic!("valid fixture identity")
            };
            assert_eq!(
                writer.abort_async().await.expect("abort"),
                if stage == TestStage::AfterPutBeforeResult {
                    WriteAbortOutcome::Published
                } else {
                    WriteAbortOutcome::NotPublished
                }
            );
        }
        assert_eq!(operation.written_bytes(), confirmed);
        assert_eq!(
            operation.state(),
            AsyncWriteAllOperationState::Failed(WriteFailureState::Indeterminate)
        );
        assert_eq!(
            control.put_calls(),
            usize::from(stage == TestStage::AfterPutBeforeResult)
        );
        if stage == TestStage::AfterPutBeforeResult {
            assert_eq!(
                store
                    .get(&key)
                    .await
                    .expect("abort preserves published target")
                    .bytes()
                    .await
                    .expect("bytes"),
                b"payload".as_slice()
            );
        }
    }
}

/// Cleanup cancellation and one-shot failure never delete a confirmed
/// publication.
#[tokio::test]
async fn sdk_result_suppression_then_abort_failure_retains_target_and_snapshot() {
    use qubit_fs::write::AsyncWriterRecovery;
    use qubit_fs::write::WriteAbortOutcome;
    use qubit_fs_s3_contract::TestStage;
    let (fs, store, control) = controlled();
    control.arm(TestStage::AfterPutBeforeResult);
    let mut operation = fs
        .begin_write_all(
            Path::parse_literal("published").expect("path"),
            b"published".to_vec(),
            WriteOptions::default().with_disposition(WriteDisposition::CreateNew),
        )
        .expect("operation");
    let mut execute = Box::pin(operation.execute());
    reach(execute.as_mut(), &control).await;
    drop(execute);
    control.release();
    let snapshot = operation.state();
    let bytes = operation.written_bytes();
    let Some(AsyncWriterRecovery::Opened(mut writer)) = operation.take_recovery() else {
        panic!("owned writer")
    };
    drop(writer.abort_async());
    assert_eq!(control.abort_calls(), 0);
    control.arm(TestStage::Abort);
    let mut abort = Box::pin(writer.abort_async());
    reach(abort.as_mut(), &control).await;
    drop(abort);
    control.release();
    control.fail_next_abort();
    assert!(writer.abort_async().await.is_err());
    assert_eq!(
        writer.abort_async().await.expect("explicit retry"),
        WriteAbortOutcome::Published
    );
    assert_eq!(control.abort_calls(), 3);
    assert_eq!(control.put_calls(), 1);
    assert_eq!(operation.state(), snapshot);
    assert_eq!(operation.written_bytes(), bytes);
    let key = ObjectPath::from("recovery/published");
    assert_eq!(
        store
            .get(&key)
            .await
            .expect("published target retained")
            .bytes()
            .await
            .expect("bytes"),
        b"published".as_slice()
    );
}

/// Empty and beyond-EOF windows still validate resource existence.
#[tokio::test]
async fn empty_ranges_keep_full_metadata_and_missing_errors() {
    use qubit_fs::read::ReadOptions;
    let (fs, store) = setup();
    let key = ObjectPath::from("recovery/range");
    store
        .put(&key, Bytes::from_static(b"abcdef").into())
        .await
        .expect("seed");
    let path = Path::parse_literal("range").expect("path");
    for (offset, length, expected) in [
        (0, Some(0), b"".as_slice()),
        (6, Some(1), b"".as_slice()),
        (7, None, b"".as_slice()),
        (u64::MAX, None, b"".as_slice()),
        (4, Some(8), b"ef".as_slice()),
    ] {
        let options = ReadOptions::default().with_offset(Some(offset)).with_length(length);
        let reader = fs.open_reader(&path, options.clone()).await.expect("range reader");
        assert_eq!(reader.info().metadata().expect("metadata").len(), Some(6));
        assert_eq!(fs.read_all(&path, options, 32).await.expect("range bytes"), expected);
    }
    let error = fs
        .open_reader(
            &Path::parse_literal("missing").expect("path"),
            ReadOptions::default().with_length(Some(0)),
        )
        .await
        .expect_err("missing");
    assert_eq!(error.kind(), qubit_fs::error::FsErrorKind::NotFound);
}
