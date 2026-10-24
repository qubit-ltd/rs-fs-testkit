// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use object_store::Error as StoreError;
use qubit_fs::error::FsError;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;

/// Converts an object store failure into the filesystem error contract.
///
/// # Parameters
///
/// * `error` - The object store failure to retain as the error source.
/// * `operation` - The filesystem operation that encountered the failure.
///
/// # Returns
///
/// The corresponding filesystem error, with the original store failure kept
/// as its source.
///
/// The mapped kind preserves recognized absence, precondition, existence,
/// authentication, and permission failures. Other store failures are mapped
/// to I/O errors.
pub fn map(error: StoreError, operation: FsOperation) -> FsError {
    let kind = match &error {
        StoreError::NotFound { .. } => FsErrorKind::NotFound,
        StoreError::Precondition { .. } => FsErrorKind::PreconditionFailed,
        StoreError::AlreadyExists { .. } => FsErrorKind::AlreadyExists,
        StoreError::Unauthenticated { .. } => FsErrorKind::AuthenticationFailed,
        StoreError::PermissionDenied { .. } => FsErrorKind::PermissionDenied,
        _ => FsErrorKind::Io,
    };
    FsError::with_source(kind, operation, "S3 operation failed", error)
}
