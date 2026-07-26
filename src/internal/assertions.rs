// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Structured filesystem error assertions.

use qubit_fs::{FileSystemCapability, FsError, FsErrorKind, FsOperation, FsPath};

/// Checks all structured fields required by one filesystem contract.
///
/// # Parameters
///
/// * `error` - Actual provider error.
/// * `kind` - Required error classification.
/// * `operation` - Required public operation.
/// * `path` - Required path when the contract is path-scoped.
/// * `provider` - Required provider identifier when provider context is
///   available.
/// * `capability` - Required capability when the error reports one.
///
/// # Panics
///
/// Panics when any expected field differs from the actual error.
pub(crate) fn assert_error(
    error: &FsError,
    kind: FsErrorKind,
    operation: FsOperation,
    path: Option<&FsPath>,
    provider: Option<&str>,
    capability: Option<FileSystemCapability>,
) {
    assert_eq!(kind, error.kind(), "filesystem error kind must match");
    assert_eq!(
        operation,
        error.operation(),
        "filesystem error operation must match",
    );
    assert_eq!(path, error.path(), "filesystem error path must match");
    assert_eq!(
        provider,
        error.provider(),
        "filesystem error provider must match",
    );
    assert_eq!(
        capability,
        error.required_capability(),
        "filesystem error required capability must match",
    );
}
