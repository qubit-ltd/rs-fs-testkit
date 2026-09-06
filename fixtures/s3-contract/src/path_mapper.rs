use crate::config::S3ContractConfig;
use qubit_fs::error::{FsErrorKind, FsOperation};
use qubit_fs::path::PathSemantics;
use qubit_fs::{FsError, FsResult, Path};

pub fn map(config: &S3ContractConfig, path: &Path) -> FsResult<String> {
    if path.semantics() != PathSemantics::ObjectKey
        || path.as_str().is_empty()
        || path.as_str().contains('\0')
    {
        return Err(FsError::new(
            FsErrorKind::InvalidPath,
            FsOperation::ParsePath,
            "S3 path must be a non-empty object key",
        ));
    }
    Ok(format!(
        "{}/{}",
        config.prefix.trim_end_matches('/'),
        path.as_str()
    ))
}

pub fn validate_key(key: &str) -> Result<(), &'static str> {
    if key.is_empty() || key.split('/').any(|p| p == "." || p == "..") {
        Err("invalid object key")
    } else {
        Ok(())
    }
}
