use qubit_fs::Path;
use qubit_io::AsyncOutput;
use qubit_fs_s3_contract::{S3ContractConfig, map, open_in_memory, validate_key};

#[test]
fn object_keys_are_preserved_and_prefixed() {
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
fn dot_segments_are_rejected() {
    assert!(validate_key("a/../b").is_err());
    assert!(validate_key("a/./b").is_err());
    assert!(validate_key("a/%2e%2e/b").is_ok());
}

#[test]
fn configuration_debug_does_not_expose_credentials() {
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

#[tokio::test]
async fn in_memory_adapter_runs_the_supported_contract_matrix() {
    use qubit_fs_testkit::{AsyncFileSystemContractSuite, AsyncFileSystemFixture, FixtureResult,
        FixtureSupport};
    use qubit_fs::{AsyncFileSystem, Path};
    use qubit_fs::metadata::ResourceVersion;

    struct Fixture {
        filesystem: AsyncFileSystem,
    }

    impl AsyncFileSystemFixture for Fixture {
        fn file_system(&self) -> &AsyncFileSystem { &self.filesystem }
        fn path(&self, relative: &str) -> FixtureResult<Path> {
            Path::parse_literal(relative).map_err(|e| qubit_fs_testkit::FixtureError::with_source(
                "fixture path failed", e))
        }
        fn seed_file<'a>(&'a self, relative: &'a str, bytes: &'a [u8])
            -> qubit_fs_testkit::FixtureFuture<'a, FixtureSupport<Path>> {
            Box::pin(async move {
                let path = self.path(relative)?;
                let options = qubit_fs::write::WriteOptions::default()
                    .with_disposition(qubit_fs::write::WriteDisposition::CreateNew);
                self.filesystem.write_all(&path, bytes, options).await
                    .map_err(|e| qubit_fs_testkit::FixtureError::new(format!("seed failed: {e}")))?;
                Ok(FixtureSupport::Supported(path))
            })
        }
        fn read_file<'a>(&'a self, path: &'a Path)
            -> qubit_fs_testkit::FixtureFuture<'a, FixtureSupport<Vec<u8>>> {
            Box::pin(async move {
                self.filesystem.read_all(path, Default::default(), 2 * 1024 * 1024).await
                    .map(FixtureSupport::Supported)
                    .map_err(|e| qubit_fs_testkit::FixtureError::new(format!("read failed: {e}")))
            })
        }
        fn resource_version<'a>(&'a self, path: &'a Path)
            -> qubit_fs_testkit::FixtureFuture<'a, FixtureSupport<ResourceVersion>> {
            Box::pin(async move {
                let metadata = self.filesystem.stat(path).await
                    .map_err(|e| qubit_fs_testkit::FixtureError::new(format!("stat failed: {e}")))?;
                Ok(metadata.etag().cloned()
                    .map(FixtureSupport::Supported).unwrap_or(FixtureSupport::Unsupported))
            })
        }
    }

    let fixture = Fixture { filesystem: open_in_memory("contract-run").unwrap() };
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    suite.assert_properties().await;
    suite.assert_stat().await;
    suite.assert_read().await;
    // The adapter deliberately exposes create-only writes. The full write
    // phase uses replace-by-default options, so its dedicated tests below
    // cover the supported writer contract instead.
    suite.finish().await;
}

#[tokio::test]
async fn create_only_collision_is_not_published_and_keeps_payload() {
    use qubit_fs::write::{WriteDisposition, WriteOptions, WriteFailureState};

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
    assert_eq!(filesystem.read_all(&path, Default::default(), 64).await.unwrap(), b"existing");
    let retry = second.commit_async().await.unwrap_err();
    assert_eq!(retry.state(), WriteFailureState::NotPublished);
}

#[tokio::test]
async fn empty_object_reports_zero_length() {
    use qubit_fs::write::{WriteDisposition, WriteOptions};

    let filesystem = open_in_memory("empty-object").unwrap();
    let path = Path::parse_literal("empty").unwrap();
    let options = WriteOptions::default().with_disposition(WriteDisposition::CreateNew);
    let mut writer = filesystem.open_writer(&path, options).await.unwrap();
    writer.commit_async().await.unwrap();
    assert_eq!(filesystem.stat(&path).await.unwrap().len(), Some(0));
}
