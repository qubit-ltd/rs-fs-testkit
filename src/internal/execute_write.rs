// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Executes prepared writes without discarding recovery ownership on failure.

use qubit_fs::AsyncFileSystem;
use qubit_fs::metadata::WriteOutcome;
use qubit_fs::path::Path;
use qubit_fs::write::WriteOptions;

use crate::ContractAsyncWriteFailure;

/// Keeps admission and execution failures distinguishable through the
/// operation.
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
