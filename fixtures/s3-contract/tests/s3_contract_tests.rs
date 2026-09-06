#[tokio::test]
#[ignore = "requires an explicitly configured, isolated S3-compatible test service"]
async fn s3_contract_requires_explicit_environment() {
    let config = qubit_fs_s3_contract::S3ContractConfig::from_env()
        .expect("S3 contract environment must be configured");
    let _filesystem =
        qubit_fs_s3_contract::open(config).expect("S3 contract configuration must be valid");
}
