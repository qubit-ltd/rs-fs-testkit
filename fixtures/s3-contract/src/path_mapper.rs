use qubit_fs::FsError;
use qubit_fs::FsResult;
use qubit_fs::Path;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::path::PathSemantics;

use crate::config::S3ContractConfig;

pub fn map(config: &S3ContractConfig, path: &Path) -> FsResult<String> {
    let prefix = config.prefix.trim_end_matches('/');
    if prefix.is_empty() || prefix.starts_with('/') || validate_key(prefix).is_err() {
        return Err(FsError::new(
            FsErrorKind::InvalidPath,
            FsOperation::ParsePath,
            "S3 prefix must be a non-empty relative object key",
        ));
    }
    if path.semantics() != PathSemantics::ObjectKey
        || path.as_str().is_empty()
        || path.as_str().contains('\0')
        || validate_key(path.as_str()).is_err()
    {
        return Err(FsError::new(
            FsErrorKind::InvalidPath,
            FsOperation::ParsePath,
            "S3 path must be a non-empty object key",
        ));
    }
    // Keep object-key spelling byte-for-byte intact: Path::parse_literal is
    // deliberately used by callers and this mapper does not normalize it.
    Ok(format!("{prefix}/{}", path.as_str()))
}

pub fn validate_key(key: &str) -> Result<(), &'static str> {
    if key.is_empty() || key.split('/').any(|p| p == "." || p == "..") {
        Err("invalid object key")
    } else {
        Ok(())
    }
}
