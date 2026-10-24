// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Helpers for temporary-directory lifecycle contracts.

use qubit_fs::error::FsErrorKind;
use qubit_fs::path::Path;
use qubit_fs::temp::AsyncTempDirectory;
use qubit_fs::temp::PersistFailureState;
use qubit_fs::temp::TempOptions;
use qubit_fs::temp::TempResourceState;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractFailure;
use crate::ContractTempFailure;
use crate::FixtureSupport;
use crate::internal::verify_condition;

/// Checks that kept and cleaned temporary directories reject repeated work.
///
/// The kept resource must preserve its published target after both rejected
/// operations; a separate cleaned resource must remain absent after a second
/// cleanup attempt.
///
/// # Parameters
///
/// * `suite` - Contract suite providing filesystem creation and observations.
/// * `id` - Repeated-lifecycle scenario used to attribute contract failures.
/// * `temporary` - Kept resource whose repeated operations are checked.
/// * `kept_target` - Published path that must remain observable.
///
/// # Returns
///
/// Returns `Ok(())` after both resource lifecycle contracts are verified.
///
/// # Errors
///
/// Returns a contract failure when an operation, state, or observation differs.
pub(super) async fn verify_repeated_temp_directory_lifecycle(
    suite: &mut AsyncFileSystemContractSuite<'_>,
    id: ContractCheckId,
    mut temporary: AsyncTempDirectory,
    kept_target: &Path,
) -> Result<(), ContractFailure> {
    let filesystem = suite.fixture.file_system();
    let failure = match temporary.keep().await {
        Err(failure) => failure,
        Ok(_) => {
            return Err(ContractFailure::message_only("temp/repeated-lifecycle: second keep succeeded").at(id));
        }
    };
    if failure.error().kind() != FsErrorKind::InvalidState
        || failure.state() != PersistFailureState::PublishedSourceReleased
        || failure.publication_target() != Some(kept_target)
    {
        return Err(ContractFailure::with_owned_source(
            "temp/repeated-lifecycle: publication facts lost",
            ContractTempFailure::new(failure, temporary),
        )
        .at(id));
    }
    let error = match temporary.cleanup().await {
        Err(error) => error,
        Ok(_) => {
            return Err(ContractFailure::message_only("temp/repeated-lifecycle: kept target reclaimed").at(id));
        }
    };
    if error.kind() != FsErrorKind::InvalidState {
        return Err(ContractFailure::with_owned_source(
            "repeated cleanup rejection differs",
            ContractTempFailure::new(error, temporary),
        )
        .at(id));
    }
    verify_condition(temporary.state() == TempResourceState::Kept, id, "kept state changed")?;
    let after =
        suite.fixture.exists_out_of_band(kept_target).await.map_err(|error| {
            ContractFailure::with_source("repeated keep publication observation failed", error).at(id)
        })?;
    verify_condition(
        matches!(after, FixtureSupport::Supported(true)),
        id,
        "kept publication disappeared",
    )?;
    let mut cleaned = filesystem
        .create_temp_directory(TempOptions::default())
        .await
        .map_err(|error| ContractFailure::with_owned_source("repeated cleanup preparation failed", error).at(id))?;
    let cleaned_source = cleaned.path().clone();
    suite.context.record_created(cleaned_source.clone());
    if let Err(error) = cleaned.cleanup().await {
        return Err(ContractFailure::with_owned_source(
            "initial cleanup failed",
            ContractTempFailure::new(error, cleaned),
        )
        .at(id));
    }
    let after = suite
        .fixture
        .exists_out_of_band(&cleaned_source)
        .await
        .map_err(|error| ContractFailure::with_source("repeated cleanup observation failed", error).at(id))?;
    verify_condition(
        matches!(after, FixtureSupport::Supported(false)),
        id,
        "cleanup retained temporary source",
    )?;
    let error = match cleaned.cleanup().await {
        Err(error) => error,
        Ok(_) => {
            return Err(ContractFailure::message_only("temp/repeated-lifecycle: cleaned resource reused").at(id));
        }
    };
    if error.kind() != FsErrorKind::InvalidState || cleaned.state() != TempResourceState::Cleaned {
        return Err(ContractFailure::with_owned_source(
            "repeated cleanup state differs",
            ContractTempFailure::new(error, cleaned),
        )
        .at(id));
    }
    Ok(())
}
