// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::env;

/// Configuration required to access an S3-compatible object store.
///
/// The access key and secret are kept out of the `Debug` representation. The
/// public fields also allow callers to construct configurations directly when
/// loading them from the environment is not appropriate.
///
/// # Examples
///
/// ```
/// use qubit_fs_s3_contract::S3ContractConfig;
///
/// let config = S3ContractConfig {
///     endpoint: "https://s3.example.com".to_owned(),
///     bucket: "documents".to_owned(),
///     region: "us-east-1".to_owned(),
///     access_key_id: "access-key".to_owned(),
///     secret_access_key: "secret-key".to_owned(),
///     prefix: "archive".to_owned(),
///     allow_http: false,
/// };
/// assert_eq!(config.bucket, "documents");
/// ```
#[derive(Clone)]
pub struct S3ContractConfig {
    /// Endpoint URL of the S3-compatible object store.
    pub endpoint: String,
    /// Bucket that stores the configured files.
    pub bucket: String,
    /// Region name passed to the object store client.
    pub region: String,
    /// Access key identifier used for authentication.
    pub access_key_id: String,
    /// Secret key used for authentication.
    pub secret_access_key: String,
    /// Relative object key prefix for this filesystem.
    pub prefix: String,
    /// Whether plain HTTP endpoints are permitted.
    pub allow_http: bool,
}

impl S3ContractConfig {
    /// Loads the S3 configuration from the `RS_FS_S3_*` environment variables.
    ///
    /// The endpoint, bucket, region, credentials, and prefix variables are
    /// required. The prefix must be representable as a relative object key.
    /// HTTP endpoints are rejected unless `RS_FS_S3_ALLOW_HTTP` is exactly
    /// `true`.
    ///
    /// # Returns
    ///
    /// The populated configuration when all required values are valid.
    ///
    /// # Errors
    ///
    /// Returns an error when a required variable is missing, the prefix is
    /// invalid, or an HTTP endpoint is used without explicit permission.
    pub fn from_env() -> Result<Self, String> {
        /// Reads a required environment variable and reports its name if absent.
        fn required(name: &str) -> Result<String, String> {
            env::var(name).map_err(|_| format!("missing required variable {name}"))
        }
        let endpoint = required("RS_FS_S3_ENDPOINT")?;
        let bucket = required("RS_FS_S3_BUCKET")?;
        let region = required("RS_FS_S3_REGION")?;
        let access_key_id = required("RS_FS_S3_ACCESS_KEY_ID")?;
        let secret_access_key = required("RS_FS_S3_SECRET_ACCESS_KEY")?;
        let prefix = required("RS_FS_S3_PREFIX")?;
        if crate::path_mapper::validate_key(&prefix).is_err() {
            return Err(
                "RS_FS_S3_PREFIX must be a non-empty, exactly representable relative key".into(),
            );
        }
        let allow_http = env::var("RS_FS_S3_ALLOW_HTTP").ok().as_deref() == Some("true");
        if endpoint.starts_with("http://") && !allow_http {
            return Err("RS_FS_S3_ALLOW_HTTP=true is required for an HTTP endpoint".into());
        }
        Ok(Self {
            endpoint,
            bucket,
            region,
            access_key_id,
            secret_access_key,
            prefix,
            allow_http,
        })
    }
}

impl std::fmt::Debug for S3ContractConfig {
    /// Formats the configuration without exposing its secret credentials.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3ContractConfig")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("prefix", &self.prefix)
            .field("allow_http", &self.allow_http)
            .finish()
    }
}
