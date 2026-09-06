use qubit_fs::Path;
use qubit_fs_s3_contract::{S3ContractConfig, map, validate_key};

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
