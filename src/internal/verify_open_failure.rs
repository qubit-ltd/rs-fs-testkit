// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Checks expected opening rejections without losing isolated recovery
//! ownership.

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::error::OpenFailure;
use qubit_fs::error::OpenFailureStage;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::path::Path;

use crate::ContractCheckId;
use crate::ContractFailure;

/// Accepts only the expected rejection before an envelope was returned.
///
/// Unexpected context, stage, or recovery ownership produces a contract failure
/// retaining the complete original opening failure. Callers can take its source
/// and explicitly recover; validation never silently drops an isolated session.
pub(crate) fn verify_open_failure<R: Send + 'static>(
    failure: OpenFailure<R>,
    kind: FsErrorKind,
    operation: FsOperation,
    path: &Path,
    provider: &str,
    capability: Option<FileSystemCapability>,
    check: ContractCheckId,
) -> Result<(), ContractFailure> {
    let error = failure.error();
    let valid = failure.recovery().is_none()
        && failure.stage() != OpenFailureStage::OutcomeValidation
        && error.kind() == kind
        && error.operation() == operation
        && error.path() == Some(path)
        && error.required_capability() == capability
        && error.provider().is_none_or(|actual| actual == provider);
    if valid {
        Ok(())
    } else {
        Err(ContractFailure::with_owned_source(
            format!("{check}: opening rejection differs from the contract"),
            failure,
        )
        .at(check))
    }
}
