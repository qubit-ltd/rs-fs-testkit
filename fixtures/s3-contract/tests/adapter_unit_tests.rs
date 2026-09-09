use qubit_fs::Path;
use qubit_fs::error::FsErrorKind;
use qubit_fs_s3_contract::S3ContractConfig;
use qubit_fs_s3_contract::map;
use qubit_fs_s3_contract::open_in_memory;
use qubit_fs_s3_contract::validate_key;
use qubit_io::AsyncOutput;

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
fn invalid_prefix_is_rejected_without_normalizing_object_keys() {
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

mod common;

/// Each supported contract runs against a fresh independent SDK fixture.
#[tokio::test]
async fn in_memory_adapter_runs_the_supported_contract_matrix() {
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
    ] {
        let fixture = common::Fixture::memory("write-contract-run");
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        suite.run_check(id).await.assert_satisfied();
    }
}

#[tokio::test]
async fn create_only_collision_is_not_published_and_keeps_payload() {
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
async fn empty_object_reports_zero_length() {
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
async fn in_memory_listing_preserves_literal_object_prefixes() {
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
async fn unsupported_writer_options_fail_before_publication() {
    use qubit_fs::write::WriteDisposition;
    use qubit_fs::write::WriteOptions;
    let filesystem = open_in_memory("writer-options").unwrap();
    let path = Path::parse_literal("options").unwrap();
    let options = WriteOptions::default()
        .with_disposition(WriteDisposition::CreateNew)
        .with_content_type(Some("text/plain".into()));
    let error = filesystem.open_writer(&path, options).await.unwrap_err();
    assert_eq!(error.kind(), FsErrorKind::RequirementNotMet);
}
