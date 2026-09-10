//! Range conversion is validated against actual HTTP status errors from the
//! SDK.

use std::sync::Arc;

use object_store::ObjectStore;
use qubit_fs::Path;
use qubit_fs::error::FsErrorKind;
use qubit_fs::read::ReadOptions;
use qubit_fs_s3_contract::S3ContractConfig;
use qubit_fs_s3_contract::TestControl;
use qubit_fs_s3_contract::open_with_control;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

/// Only 416 followed by a successful HEAD proving EOF may become an empty read.
#[tokio::test]
async fn http_range_conversion_requires_metadata_evidence() {
    for (first_status, head_status, offset, empty, missing) in [
        (416, Some(200), 3, true, false),
        (416, Some(200), 2, false, false),
        (416, Some(404), 3, false, true),
        (403, None, 3, false, false),
        (500, None, 3, false, false),
        (404, None, 3, false, true),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let mut statuses = vec![first_status];
            statuses.extend(head_status);
            for (index, status) in statuses.into_iter().enumerate() {
                let (mut socket, _) = listener.accept().await.expect("request connection");
                let mut request = Vec::new();
                let mut chunk = [0; 2048];
                while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                    let count = socket.read(&mut chunk).await.expect("request headers");
                    assert!(count > 0);
                    request.extend_from_slice(&chunk[..count]);
                }
                let headers = String::from_utf8(request).expect("HTTP headers").to_ascii_lowercase();
                if index == 0 {
                    assert!(headers.starts_with("get "));
                    assert!(headers.contains("range: bytes="));
                } else {
                    assert!(headers.starts_with("head "));
                    assert!(!headers.contains("range:"));
                }
                let length = if status == 200 { 3 } else { 0 };
                let reason = match status {
                    200 => "OK",
                    403 => "Forbidden",
                    404 => "Not Found",
                    416 => "Range Not Satisfiable",
                    _ => "Internal Server Error",
                };
                let response = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Length: {length}\r\nETag: \"version\"\r\nLast-Modified: Wed, 21 Oct 2015 07:28:00 GMT\r\nConnection: close\r\n\r\n"
                );
                socket.write_all(response.as_bytes()).await.expect("response");
            }
        });
        let config = S3ContractConfig {
            endpoint: format!("http://{address}"),
            bucket: "fixture".into(),
            region: "test-region".into(),
            access_key_id: "test".into(),
            secret_access_key: "test".into(),
            prefix: "ranges".into(),
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
                .expect("SDK client"),
        );
        let filesystem = open_with_control(config, store, TestControl::default()).expect("facade");
        let result = filesystem
            .read_all(
                &Path::parse_literal("key").expect("path"),
                ReadOptions::default().with_offset(Some(offset)).with_length(Some(1)),
                1,
            )
            .await;
        if empty {
            assert!(result.expect("confirmed EOF").is_empty());
        } else {
            let error = result.expect_err("range failure must not become empty success");
            if missing {
                assert_eq!(error.kind(), FsErrorKind::NotFound);
            }
            if first_status == 403 {
                assert_eq!(error.kind(), FsErrorKind::PermissionDenied);
            }
            assert!(std::error::Error::source(&error).is_some());
        }
        server.await.expect("expected HTTP requests completed");
    }
}
