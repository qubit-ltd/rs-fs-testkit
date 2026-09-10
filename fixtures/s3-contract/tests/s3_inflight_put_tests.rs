//! Deterministic local HTTP boundary evidence for a PUT awaiting its response.

use std::sync::Arc;

use object_store::ObjectStore;
use qubit_fs::Path;
use qubit_fs::write::AsyncWriterRecovery;
use qubit_fs::write::WriteAbortOutcome;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteFailureState;
use qubit_fs::write::WriteOptions;
use qubit_fs_s3_contract::S3ContractConfig;
use qubit_fs_s3_contract::TestControl;
use qubit_fs_s3_contract::open_with_control;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

/// Cancellation after the HTTP request was received cannot prove NotPublished.
#[tokio::test]
async fn cancelled_inflight_put_remains_indeterminate_during_abort() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("local listener");
    let address = listener.local_addr().expect("address");
    let (received_tx, received_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("connection");
        let mut request = Vec::new();
        let mut chunk = [0; 4096];
        loop {
            let count = socket.read(&mut chunk).await.expect("request read");
            assert!(count > 0, "request ended before headers and body");
            request.extend_from_slice(&chunk[..count]);
            if let Some(boundary) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..boundary]).to_ascii_lowercase();
                let length: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .expect("content length")
                    .trim()
                    .parse()
                    .expect("numeric length");
                if request.len() >= boundary + 4 + length {
                    break;
                }
            }
        }
        received_tx.send(()).expect("request acknowledgement");
        release_rx.await.expect("explicit response release");
        // Cancellation may close the connection. Neither outcome is publication
        // evidence.
        let _response = socket
            .write_all(b"HTTP/1.1 200 OK\r\nETag: \"stored\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
    });
    let config = S3ContractConfig {
        endpoint: format!("http://{address}"),
        bucket: "fixture".into(),
        region: "test-region".into(),
        access_key_id: "test".into(),
        secret_access_key: "test".into(),
        prefix: "inflight".into(),
        allow_http: true,
    };
    let store: Arc<dyn ObjectStore> = Arc::new(
        object_store::aws::AmazonS3Builder::new()
            .with_endpoint(&config.endpoint)
            .with_allow_http(true)
            .with_bucket_name(&config.bucket)
            .with_region(&config.region)
            .with_access_key_id(&config.access_key_id)
            .with_secret_access_key(&config.secret_access_key)
            .with_retry(object_store::RetryConfig {
                max_retries: 0,
                ..Default::default()
            })
            .build()
            .expect("mock SDK client"),
    );
    let control = TestControl::default();
    let fs = open_with_control(config, store, control.clone()).expect("facade");
    let mut operation = fs
        .begin_write_all(
            Path::parse_literal("key").expect("path"),
            b"payload".to_vec(),
            WriteOptions::default().with_disposition(WriteDisposition::CreateNew),
        )
        .expect("operation");
    let mut execute = Box::pin(operation.execute());
    tokio::select! {
        result = &mut execute => panic!("operation completed before held HTTP response: {result:?}"),
        received = received_rx => received.expect("server received request"),
    }
    drop(execute);
    assert_eq!(operation.written_bytes(), 7);
    let Some(AsyncWriterRecovery::Opened(mut writer)) = operation.take_recovery() else {
        panic!("retained session")
    };
    assert_eq!(
        writer.abort_async().await.expect("abort observation"),
        WriteAbortOutcome::Indeterminate
    );
    let failure = operation.execute().await.expect_err("no repeated execution");
    assert_eq!(failure.state(), WriteFailureState::Indeterminate);
    assert_eq!(control.put_calls(), 1);
    release_tx.send(()).expect("release server");
    server.await.expect("server finished");
}
