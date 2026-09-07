// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Keeps copy failure snapshots paired with their owning operation.

use qubit_fs::AsyncFileSystem;
use qubit_fs::copy::CopyOptions;
use qubit_fs::copy::CopyOutcome;
use qubit_fs::path::Path;

use crate::ContractAsyncCopyFailure;

/// Preserves admission or execution failures without losing recovery ownership.
pub(crate) async fn execute_copy(
    filesystem: &AsyncFileSystem,
    source: Path,
    target: Path,
    options: CopyOptions,
) -> Result<CopyOutcome, ContractAsyncCopyFailure> {
    let mut operation = filesystem
        .begin_copy(source, target, options)
        .map_err(|failure| ContractAsyncCopyFailure::new(failure, None))?;
    match operation.execute().await {
        Ok(outcome) => Ok(outcome),
        Err(failure) => Err(ContractAsyncCopyFailure::new(failure, Some(operation))),
    }
}
