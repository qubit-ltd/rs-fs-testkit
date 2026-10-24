// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use bytes::Bytes;
use futures_util::stream;
use object_store::ObjectStoreExt;
use object_store::memory::InMemory;
use object_store::path::Path as ObjectStorePath;
use qubit_fs::directory::ListScope;
use qubit_fs::error::FsErrorKind;
use qubit_fs::spi::AsyncDirectoryStreamSession;

use crate::S3ContractConfig;
use crate::s3_directory_stream::S3DirectoryStream;

/// Even a faulty SDK stream cannot return entries beyond configured scope.
#[tokio::test]
async fn test_rejects_sdk_entry_outside_namespace() {
    let store = InMemory::new();
    let key = ObjectStorePath::from("foreign/key");
    store.put(&key, Bytes::from_static(b"x").into()).await.unwrap();
    let meta = store.head(&key).await.unwrap();
    let config = S3ContractConfig {
        endpoint: "memory://".into(),
        bucket: "test".into(),
        region: "local".into(),
        access_key_id: "test".into(),
        secret_access_key: "test".into(),
        prefix: "owned".into(),
        allow_http: false,
    };
    let mut stream = S3DirectoryStream::new(
        config,
        ListScope::Namespace,
        Box::pin(stream::iter([Ok(meta)])),
        String::new(),
        false,
    );
    let error = stream.next_entry_async().await.unwrap_err();
    assert_eq!(error.kind(), FsErrorKind::ProviderContractViolation);
    assert_eq!(error.path(), None);
}
