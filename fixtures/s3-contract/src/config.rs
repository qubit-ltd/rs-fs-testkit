use std::env;

#[derive(Clone)]
pub struct S3ContractConfig {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub prefix: String,
    pub allow_http: bool,
}

impl S3ContractConfig {
    pub fn from_env() -> Result<Self, String> {
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
            return Err("RS_FS_S3_PREFIX must be a non-empty, exactly representable relative key".into());
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
