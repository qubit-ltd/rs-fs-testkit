// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
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
mod common;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

/// Cancellation after the HTTP request was received cannot prove NotPublished.
#[tokio::test]
async fn test_cancelled_inflight_put_remains_indeterminate_during_abort() {
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
    common::http_support::run_http_scenario(
        "inflight PUT ACK / response",
        server,
        move || {
            let _ = release_tx.send(());
        },
        async move {
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
            Ok(())
        },
    )
    .await
    .expect("inflight PUT completed and server was joined");
}

#[tokio::test]
async fn test_http_put_without_ack_reports_deadline_and_reaps_server() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let (request_received_tx, request_received_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("PUT connection");
        let mut request = Vec::new();
        let mut chunk = [0; 4096];
        loop {
            let count = socket.read(&mut chunk).await.expect("PUT request");
            assert!(count > 0, "PUT ended before its complete body");
            request.extend_from_slice(&chunk[..count]);
            if let Some(boundary) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..boundary]).to_ascii_lowercase();
                assert!(headers.starts_with("put "), "{headers}");
                let length: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .expect("content length")
                    .trim()
                    .parse()
                    .expect("numeric content length");
                if request.len() >= boundary + 4 + length {
                    break;
                }
            }
        }
        request_received_tx
            .send(())
            .expect("controlled deadline waits for complete PUT");
        // Keep the connection open without returning an HTTP acknowledgement.
        std::future::pending::<()>().await;
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
    let filesystem = open_with_control(config, store, TestControl::default()).expect("facade");
    let mut operation = filesystem
        .begin_write_all(
            Path::parse_literal("key").expect("path"),
            b"payload".to_vec(),
            WriteOptions::default().with_disposition(WriteDisposition::CreateNew),
        )
        .expect("write operation");
    let result = common::http_support::run_http_scenario_until(
        "inflight PUT ACK / response",
        server,
        || {},
        async move {
            let result = operation.execute().await;
            Err(format!("PUT completed without an ACK deadline: {result:?}"))
        },
        async move {
            request_received_rx
                .await
                .expect("server must receive complete PUT before deadline");
        },
        "PUT response deadline reached after complete request",
    )
    .await;
    let error = result.expect_err("unacknowledged PUT must reach its controlled deadline");
    assert!(error.contains("inflight PUT ACK / response"), "{error}");
    assert!(
        error.contains("PUT response deadline reached after complete request"),
        "{error}"
    );
    assert!(error.contains("server task reaped"), "{error}");
}

#[tokio::test(start_paused = true)]
async fn test_http_scenario_reaps_server_pending_after_successful_scenario() {
    let server = tokio::spawn(async move {
        std::future::pending::<()>().await;
    });
    let result =
        common::http_support::run_http_scenario("completed PUT with pending server", server, || {}, async { Ok(()) })
            .await;
    let error = result.expect_err("permanently pending server must hit its join deadline");
    assert!(error.contains("completed PUT with pending server"), "{error}");
    assert!(error.contains("server join exceeded 5 second deadline"), "{error}");
    assert!(error.contains("server task reaped"), "{error}");
}
