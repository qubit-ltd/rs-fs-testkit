// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Executes prepared writes without discarding recovery ownership on failure.

use qubit_fs::AsyncFileSystem;
use qubit_fs::metadata::WriteOutcome;
use qubit_fs::path::Path;
use qubit_fs::write::WriteOptions;

use crate::ContractAsyncWriteFailure;

/// Executes a prepared whole-file write while retaining its recovery operation
/// when execution fails.
///
/// Admission failures are returned without an operation because no write
/// session was created. Execution failures retain the operation, including
/// any recovery writer, in [`ContractAsyncWriteFailure`].
///
/// # Parameters
///
/// - `filesystem`: filesystem that admits and executes the write.
/// - `path`: destination path for the complete byte buffer.
/// - `bytes`: contents to publish at the destination.
/// - `options`: write disposition and other write policy.
///
/// # Returns
///
/// The write outcome when execution succeeds; otherwise, the failure snapshot
/// and any admitted operation needed for recovery.
///
/// # Errors
///
/// Returns [`ContractAsyncWriteFailure`] if admission or execution fails.
pub(crate) async fn execute_write(
    filesystem: &AsyncFileSystem,
    path: Path,
    bytes: Vec<u8>,
    options: WriteOptions,
) -> Result<WriteOutcome, ContractAsyncWriteFailure> {
    let mut operation = filesystem
        .begin_write_all(path, bytes, options)
        .map_err(|failure| ContractAsyncWriteFailure::new(failure, None))?;
    match operation.execute().await {
        Ok(outcome) => Ok(outcome),
        Err(failure) => Err(ContractAsyncWriteFailure::new(failure, Some(operation))),
    }
}
