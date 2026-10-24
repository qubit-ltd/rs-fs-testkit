// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use object_store::path::Path as ObjectPath;
use qubit_fs::FsError;
use qubit_fs::FsResult;
use qubit_fs::Path;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::path::PathSemantics;

use crate::config::S3ContractConfig;

/// Maps a filesystem object key into the configured S3 namespace.
///
/// The path must use object-key semantics and contain a valid, non-empty key.
/// Its spelling is preserved while the configured prefix is prepended.
///
/// # Errors
///
/// Returns an error when the configured prefix or path is not a valid object
/// key.
pub fn map(config: &S3ContractConfig, path: &Path) -> FsResult<String> {
    let prefix = config.prefix.as_str();
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

/// Rejects resource keys whose SDK representation would change their identity.
///
/// This checks that the key is non-empty, contains no NUL byte, and round-trips
/// through the object store SDK without normalization.
///
/// # Errors
///
/// Returns a static message when the key is invalid or its spelling would
/// change in the SDK representation.
pub fn validate_key(key: &str) -> Result<(), &'static str> {
    if key.is_empty() || key.contains('\0') {
        return Err("invalid object key");
    }
    let parsed = ObjectPath::parse(key).map_err(|_| "invalid object key")?;
    if parsed.as_ref() != key {
        return Err("object key is not preserved by the SDK");
    }
    Ok(())
}

/// Validates the configured namespace independently of a caller's query prefix.
///
/// The returned SDK path represents the configured prefix after validation.
///
/// # Errors
///
/// Returns an `InvalidOptions` error when the prefix is invalid or cannot be
/// parsed by the SDK.
pub(crate) fn configured_prefix(config: &S3ContractConfig) -> FsResult<ObjectPath> {
    validate_key(&config.prefix).map_err(|message| {
        FsError::new(FsErrorKind::InvalidOptions, FsOperation::Provider, message)
    })?;
    ObjectPath::parse(&config.prefix).map_err(|error| {
        FsError::with_source(
            FsErrorKind::InvalidOptions,
            FsOperation::Provider,
            "invalid configured prefix",
            error,
        )
    })
}
