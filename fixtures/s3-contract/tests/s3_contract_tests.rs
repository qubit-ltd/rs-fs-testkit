// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Opt-in real-service I/O evidence with cleanup limited to this run's keys.

use std::collections::HashSet;
use std::io;
use std::process;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bytes::Bytes;
use object_store::ObjectStore;
use object_store::ObjectStoreExt;
use object_store::PutMode;
use object_store::PutOptions;
use object_store::RetryConfig;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path as ObjectStorePath;
use qubit_fs::Path;
use qubit_fs::directory::ListOptions;
use qubit_fs::directory::ListScope;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteOptions;
use qubit_fs_s3_contract::S3ContractConfig;
use qubit_fs_s3_contract::open_with_store;
use qubit_fs_testkit::AsyncFileSystemFixture;

/// Performs actual PUT, GET, HEAD, and LIST against an explicitly configured
/// service.
#[tokio::test]
#[ignore = "requires an explicitly configured, isolated S3-compatible test service"]
async fn test_s3_contract_requires_explicit_environment() {
    let mut config = S3ContractConfig::from_env().expect("explicit S3 configuration required");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    config.prefix = format!("{}/io-{}-{nonce}", config.prefix, process::id());
    let store: Arc<dyn ObjectStore> = Arc::new(
        AmazonS3Builder::new()
            .with_endpoint(&config.endpoint)
            .with_allow_http(config.allow_http)
            .with_bucket_name(&config.bucket)
            .with_region(&config.region)
            .with_access_key_id(&config.access_key_id)
            .with_secret_access_key(&config.secret_access_key)
            .with_retry(RetryConfig {
                max_retries: 0,
                ..Default::default()
            })
            .build()
            .expect("valid explicit service configuration"),
    );
    let filesystem = open_with_store(config.clone(), Arc::clone(&store)).unwrap();
    let names = ["folder", "folder/a", "folderish"];
    let candidates = names
        .iter()
        .map(|name| ObjectStorePath::parse(format!("{}/{name}", config.prefix)).unwrap())
        .collect::<Vec<_>>();
    let owned = Arc::new(Mutex::new(HashSet::new()));
    let ledger = Arc::clone(&owned);
    let store_for_verification = Arc::clone(&store);
    let unresolved_candidates = candidates.clone();
    let (result, cleanup_errors) = common::exact_key_cleanup::run_with_exact_key_cleanup_ledger(
        store.as_ref(),
        Arc::clone(&owned),
        async move {
            for key in &candidates[..2] {
                store_for_verification
                    .put_opts(
                        key,
                        Bytes::from_static(b"real-s3").into(),
                        PutOptions {
                            mode: PutMode::Create,
                            ..Default::default()
                        },
                    )
                    .await?;
                ledger
                    .lock()
                    .expect("created-key ledger")
                    .insert(key.clone());
            }
            let target = Path::parse_literal(names[2])?;
            let mut write = filesystem.begin_write_all(
                target.clone(),
                b"real-s3".to_vec(),
                WriteOptions::default().with_disposition(WriteDisposition::CreateNew),
            )?;
            write.execute().await?;
            ledger
                .lock()
                .expect("created-key ledger")
                .insert(candidates[2].clone());
            let stored = store_for_verification
                .get(&candidates[2])
                .await?
                .bytes()
                .await?;
            if stored.as_ref() != b"real-s3" {
                return Err(io::Error::other("SDK GET did not observe facade PUT bytes").into());
            }
            for name in names {
                let path = Path::parse_literal(name)?;
                if filesystem.stat(&path).await?.len() != Some(7) {
                    return Err(io::Error::other("HEAD length mismatch").into());
                }
                if filesystem.read_all(&path, Default::default(), 64).await? != b"real-s3" {
                    return Err(io::Error::other("GET content mismatch").into());
                }
            }
            for scope in [
                ListScope::Namespace,
                ListScope::Path(Path::parse_literal("folder")?),
            ] {
                let mut stream = filesystem.list(&scope, ListOptions::object_keys()).await?;
                let mut actual = Vec::new();
                while let Some(entry) = stream.next_entry_async().await? {
                    actual.push(entry.path.as_str().to_owned());
                }
                actual.sort();
                if actual != names {
                    return Err(io::Error::other("LIST exact-key set mismatch").into());
                }
            }
            Ok(())
        },
    )
    .await;
    let confirmed = owned.lock().expect("created-key ledger");
    let unresolved = unresolved_candidates
        .iter()
        .filter(|key| !confirmed.contains(*key))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if !unresolved.is_empty() {
        eprintln!(
            "candidate keys without confirmed creation evidence were retained: {}",
            unresolved.join(", ")
        );
    }
    common::exact_key_cleanup::finalize_verification(result, cleanup_errors);
}

mod common;

/// Runs real conditional writes, stage probes and SDK-boundary result
/// suppression. This injection does not claim to simulate packet loss or
/// multipart cleanup.
#[tokio::test]
#[ignore = "requires explicit RS_FS_S3_* configuration; writes exact keys under a unique prefix"]
async fn test_real_s3_recovery_matrix_preserves_published_targets() {
    let mut config = S3ContractConfig::from_env().expect("explicit S3 configuration required");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    config.prefix = format!("{}/recovery-{}-{nonce}", config.prefix, process::id());
    let store: Arc<dyn ObjectStore> = Arc::new(
        AmazonS3Builder::new()
            .with_endpoint(&config.endpoint)
            .with_allow_http(config.allow_http)
            .with_bucket_name(&config.bucket)
            .with_region(&config.region)
            .with_access_key_id(&config.access_key_id)
            .with_secret_access_key(&config.secret_access_key)
            .with_retry(RetryConfig {
                max_retries: 0,
                ..Default::default()
            })
            .build()
            .expect("explicit service client"),
    );
    let fixture = common::Fixture::new(config, store);
    common::failure_preservation::verify_then_cleanup(
        common::recovery_matrix::verify(&fixture),
        async {
            fixture
                .teardown()
                .await
                .map_err(|error| io::Error::other(error.to_string()))
        },
        |diagnostic| eprintln!("{diagnostic}"),
    )
    .await;
}
