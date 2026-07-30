// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow source-test-pair
//! Structured filesystem error assertions.

use qubit_fs::{
    FileSystemCapability,
    FsError,
    FsErrorKind,
    FsOperation,
    Path,
};

/// Checks all structured fields required by one filesystem contract.
///
/// # Parameters
///
/// * `error` - Actual provider error.
/// * `kind` - Required error classification.
/// * `operation` - Required public operation.
/// * `path` - Required path when the contract is path-scoped.
/// * `provider` - Expected provider identifier when the error reports provider
///   context.
/// * `capability` - Required capability when the error reports one.
///
/// # Panics
///
/// Panics when any expected field differs from the actual error.
#[track_caller]
pub(crate) fn assert_error(
    error: &FsError,
    kind: FsErrorKind,
    operation: FsOperation,
    path: Option<&Path>,
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
    assert!(
        error.provider().is_none() || error.provider() == provider,
        "filesystem error provider must be absent or match the configured provider",
    );
    assert_eq!(
        capability,
        error.required_capability(),
        "filesystem error required capability must match",
    );
}

/// Checks all structured fields for an error that includes a destination path.
///
/// # Parameters
///
/// * `error` - Actual provider error.
/// * `kind` - Required error classification.
/// * `operation` - Required public operation.
/// * `path` - Required source path.
/// * `target` - Required destination path.
/// * `provider` - Required provider identifier.
/// * `capability` - Required capability when the error reports one.
///
/// # Panics
///
/// Panics when any expected field differs from the actual error.
#[track_caller]
pub(crate) fn assert_error_with_target(
    error: &FsError,
    kind: FsErrorKind,
    operation: FsOperation,
    path: Option<&Path>,
    target: Option<&Path>,
    provider: Option<&str>,
    capability: Option<FileSystemCapability>,
) {
    assert_error(error, kind, operation, path, provider, capability);
    assert_eq!(target, error.target(), "filesystem error target must match");
}

/// Checks the required fields of an unsupported-operation error.
///
/// # Parameters
///
/// * `error` - Actual provider error.
/// * `kind` - Required error classification.
/// * `operation` - Required public operation.
/// * `path` - Required source path.
/// * `provider` - Configured provider identifier, when one is available.
/// * `capability` - Required unsupported capability.
///
/// # Panics
///
/// Panics when a required field differs, or when a reported provider does not
/// match the configured provider. Missing provider context remains valid for
/// trait-default errors.
#[track_caller]
pub(crate) fn assert_unsupported_error(
    error: &FsError,
    kind: FsErrorKind,
    operation: FsOperation,
    path: Option<&Path>,
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
        capability,
        error.required_capability(),
        "filesystem error required capability must match",
    );
    assert!(
        error.provider().is_none() || error.provider() == provider,
        "filesystem error provider must be absent or match the configured provider",
    );
}
