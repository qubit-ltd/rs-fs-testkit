//! Opt-in real-service I/O evidence with cleanup limited to this run's keys.

use std::sync::Arc;

use object_store::ObjectStore;
use object_store::ObjectStoreExt;
use qubit_fs::Path;
use qubit_fs::directory::ListOptions;
use qubit_fs::directory::ListScope;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteOptions;

/// Performs actual PUT, GET, HEAD, and LIST against an explicitly configured
/// service.
#[tokio::test]
#[ignore = "requires an explicitly configured, isolated S3-compatible test service"]
async fn s3_contract_requires_explicit_environment() {
    let mut config = qubit_fs_s3_contract::S3ContractConfig::from_env().expect("explicit S3 configuration required");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    config.prefix = format!("{}/io-{}-{nonce}", config.prefix, std::process::id());
    let store: Arc<dyn ObjectStore> = Arc::new(
        object_store::aws::AmazonS3Builder::new()
            .with_endpoint(&config.endpoint)
            .with_allow_http(config.allow_http)
            .with_bucket_name(&config.bucket)
            .with_region(&config.region)
            .with_access_key_id(&config.access_key_id)
            .with_secret_access_key(&config.secret_access_key)
            .with_retry(object_store::RetryConfig {
                max_retries: 0,
                ..Default::default()
            })
            .build()
            .expect("valid explicit service configuration"),
    );
    let filesystem = qubit_fs_s3_contract::open_with_store(config.clone(), Arc::clone(&store)).unwrap();
    let names = ["folder", "folder/a", "folderish"];
    let owned = names
        .iter()
        .map(|name| object_store::path::Path::parse(format!("{}/{name}", config.prefix)).unwrap())
        .collect::<Vec<_>>();
    let result: Result<(), Box<dyn std::error::Error>> = async {
        for key in &owned[..2] {
            store
                .put_opts(
                    key,
                    bytes::Bytes::from_static(b"real-s3").into(),
                    object_store::PutOptions {
                        mode: object_store::PutMode::Create,
                        ..Default::default()
                    },
                )
                .await?;
        }
        let target = Path::parse_literal(names[2])?;
        let mut write = filesystem.begin_write_all(
            target.clone(),
            b"real-s3".to_vec(),
            WriteOptions::default().with_disposition(WriteDisposition::CreateNew),
        )?;
        write.execute().await?;
        let stored = store.get(&owned[2]).await?.bytes().await?;
        if stored.as_ref() != b"real-s3" {
            return Err(std::io::Error::other("SDK GET did not observe facade PUT bytes").into());
        }
        for name in names {
            let path = Path::parse_literal(name)?;
            if filesystem.stat(&path).await?.len() != Some(7) {
                return Err(std::io::Error::other("HEAD length mismatch").into());
            }
            if filesystem.read_all(&path, Default::default(), 64).await? != b"real-s3" {
                return Err(std::io::Error::other("GET content mismatch").into());
            }
        }
        for scope in [ListScope::Namespace, ListScope::Path(Path::parse_literal("folder")?)] {
            let mut stream = filesystem.list(&scope, ListOptions::object_keys()).await?;
            let mut actual = Vec::new();
            while let Some(entry) = stream.next_entry_async().await? {
                actual.push(entry.path.as_str().to_owned());
            }
            actual.sort();
            if actual != names {
                return Err(std::io::Error::other("LIST exact-key set mismatch").into());
            }
        }
        Ok(())
    }
    .await;
    // Always attempt cleanup before reporting I/O verification failures. These
    // exact keys are confined to this run's unique child of the configured prefix.
    let mut cleanup_errors = Vec::new();
    for key in &owned {
        if let Err(error) = store.delete(key).await
            && !matches!(error, object_store::Error::NotFound { .. })
        {
            cleanup_errors.push(error);
        }
    }
    assert!(
        cleanup_errors.is_empty(),
        "cleanup failed for {} owned keys",
        cleanup_errors.len()
    );
    result.expect("real service PUT/GET/HEAD/LIST verification failed");
}

mod common;

/// Runs real conditional writes, stage probes and SDK-boundary result
/// suppression. This injection does not claim to simulate packet loss or
/// multipart cleanup.
#[tokio::test]
#[ignore = "requires explicit RS_FS_S3_* configuration; writes exact keys under a unique prefix"]
async fn real_s3_recovery_matrix_preserves_published_targets() {
    use futures_util::FutureExt;
    use qubit_fs_testkit::AsyncFileSystemFixture;
    let mut config = qubit_fs_s3_contract::S3ContractConfig::from_env().expect("explicit S3 configuration required");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    config.prefix = format!("{}/recovery-{}-{nonce}", config.prefix, std::process::id());
    let store: Arc<dyn ObjectStore> = Arc::new(
        object_store::aws::AmazonS3Builder::new()
            .with_endpoint(&config.endpoint)
            .with_allow_http(config.allow_http)
            .with_bucket_name(&config.bucket)
            .with_region(&config.region)
            .with_access_key_id(&config.access_key_id)
            .with_secret_access_key(&config.secret_access_key)
            .with_retry(object_store::RetryConfig {
                max_retries: 0,
                ..Default::default()
            })
            .build()
            .expect("explicit service client"),
    );
    let fixture = common::Fixture::new(config, store);
    let result = std::panic::AssertUnwindSafe(common::recovery_matrix::verify(&fixture))
        .catch_unwind()
        .await;
    let cleanup = fixture.teardown().await;
    cleanup.expect("exact-key cleanup must succeed");
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
