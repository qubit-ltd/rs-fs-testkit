// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Verifies temporary-directory persistence and overwrite contracts.

use qubit_fs::AsyncFileSystem;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::AchievedAtomicity;
use qubit_fs::metadata::AtomicityRequirement;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::temp::AsyncTempDirectory;
use qubit_fs::temp::PersistFailureState;
use qubit_fs::temp::PersistOptions;
use qubit_fs::temp::TempOptions;
use qubit_fs::temp::TempResourceState;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractFailure;
use crate::ContractTempFailure;
use crate::FixtureSupport;
use crate::internal::verify_condition;

/// Publishes a temporary directory and checks the selected atomicity contract.
///
/// The temporary resource is consumed by publication; failures retain it when
/// the provider reports a recoverable resource state.
///
/// # Parameters
///
/// * `id` - Contract scenario associated with the publication.
/// * `atomic` - Whether atomic persistence is required.
/// * `temporary` - Owned temporary directory being published.
///
/// # Returns
///
/// Returns `Ok(())` after publication or expected capability rejection.
///
/// # Errors
///
/// Returns a contract failure when publication, state, or path observations
/// differ from the selected contract.
pub(super) async fn persist_temp_directory(
    suite: &mut AsyncFileSystemContractSuite<'_>,
    id: ContractCheckId,
    atomic: bool,
    mut temporary: AsyncTempDirectory,
) -> Result<(), ContractFailure> {
    let source = temporary.path().clone();
    let target = suite
        .fixture
        .path(&suite.context.relative_name("persisted-directory"))
        .map_err(|error| {
            ContractFailure::with_source("temporary publication target preparation failed", error).at(id)
        })?;
    suite.context.record_created(target.clone());
    let options = PersistOptions::default().with_atomicity(if atomic {
        AtomicityRequirement::Required
    } else {
        AtomicityRequirement::Preferred
    });
    let publication = temporary.persist(&target, options).await;
    if atomic && !suite.capable(FileSystemCapability::AtomicTempPersist) {
        let failure = match publication {
            Err(failure) => failure,
            Ok(_) => {
                return Err(ContractFailure::message_only("unsupported atomic persistence succeeded").at(id));
            }
        };
        let error = failure.error();
        if error.kind() != FsErrorKind::RequirementNotMet
            || error.operation() != FsOperation::PersistTemp
            || error.path() != Some(&source)
            || error.target() != Some(&target)
            || error.required_capability() != Some(FileSystemCapability::AtomicTempPersist)
            || error.provider() != Some(suite.context.properties().info().provider_id())
            || failure.state() != PersistFailureState::NotPublished
        {
            return Err(ContractFailure::with_owned_source(
                "atomic persistence rejection differs",
                ContractTempFailure::new(failure, temporary),
            )
            .at(id));
        }
        let before = suite.fixture.exists_out_of_band(&source).await.map_err(|error| {
            ContractFailure::with_source("atomic rejection source observation failed", error).at(id)
        })?;
        verify_condition(
            matches!(before, FixtureSupport::Supported(true)),
            id,
            "atomic preflight removed source",
        )?;
        verify_condition(
            temporary.state() == TempResourceState::Owned,
            id,
            "atomic preflight changed resource ownership",
        )?;
        let target_after = suite.fixture.exists_out_of_band(&target).await.map_err(|error| {
            ContractFailure::with_source("atomic rejection target observation failed", error).at(id)
        })?;
        verify_condition(
            matches!(target_after, FixtureSupport::Supported(false)),
            id,
            "atomic preflight published a target",
        )?;
        if let Err(error) = temporary.cleanup().await {
            return Err(ContractFailure::with_owned_source(
                "atomic rejection cleanup failed",
                ContractTempFailure::new(error, temporary),
            )
            .at(id));
        }
        let after = suite.fixture.exists_out_of_band(&source).await.map_err(|error| {
            ContractFailure::with_source("atomic rejection cleanup observation failed", error).at(id)
        })?;
        verify_condition(
            matches!(after, FixtureSupport::Supported(false)),
            id,
            "atomic rejection cleanup retained source",
        )?;
        return Ok(());
    }
    let outcome = match publication {
        Ok(outcome) => outcome,
        Err(error) => {
            return Err(ContractFailure::with_owned_source(
                "temp/atomic: persistence failed",
                ContractTempFailure::new(error, temporary),
            )
            .at(id));
        }
    };
    verify_condition(outcome.target() == &target, id, "temporary persist target mismatch")?;
    if atomic {
        verify_condition(
            outcome.atomicity() == AchievedAtomicity::Atomic,
            id,
            "temp/atomic: non-atomic publication",
        )?;
    }
    for (path, expected) in [(&source, false), (&target, true)] {
        let observed = suite
            .fixture
            .exists_out_of_band(path)
            .await
            .map_err(|error| ContractFailure::with_source("temporary persist observation failed", error).at(id))?;
        verify_condition(
            matches!(observed, FixtureSupport::Supported(actual) if actual == expected),
            id,
            "temporary persistence source release or target publication differs",
        )?;
    }
    Ok(())
}

/// Verifies overwrite publication using an independently prepared target.
///
/// # Parameters
///
/// * `id` - Basic temporary-directory scenario being checked.
///
/// # Returns
///
/// Returns `Ok(())` when the replacement is published at the prepared target.
///
/// # Errors
///
/// Returns a contract failure when fixture preparation, persistence, or path
/// observations differ from the overwrite contract.
pub(super) async fn verify_temp_directory_overwrite(
    suite: &mut AsyncFileSystemContractSuite<'_>,
    id: ContractCheckId,
) -> Result<(), ContractFailure> {
    let filesystem = suite.fixture.file_system();
    let relative = suite.context.relative_name("temp-overwritten-directory");
    let prepared =
        suite.fixture.seed_empty_directory(&relative).await.map_err(|error| {
            ContractFailure::with_source("temporary overwrite target preparation failed", error).at(id)
        })?;
    let FixtureSupport::Supported(target) = prepared else {
        return Err(ContractFailure::message_only(
            "temporary directory overwrite needs independent target preparation",
        )
        .at(id));
    };
    suite.context.record_created(target.clone());
    let mut replacement = filesystem
        .create_temp_directory(TempOptions::default())
        .await
        .map_err(|error| ContractFailure::with_owned_source("temporary overwrite preparation failed", error).at(id))?;
    let source = replacement.path().clone();
    suite.context.record_created(source.clone());
    let outcome = match replacement
        .persist(
            &target,
            PersistOptions::default()
                .with_overwrite(true)
                .with_atomicity(AtomicityRequirement::Preferred),
        )
        .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            return Err(ContractFailure::with_owned_source(
                "temporary directory overwrite failed",
                ContractTempFailure::new(error, replacement),
            )
            .at(id));
        }
    };
    verify_condition(outcome.target() == &target, id, "temporary overwrite target differs")?;
    for (path, expected) in [(&source, false), (&target, true)] {
        let observed =
            suite.fixture.exists_out_of_band(path).await.map_err(|error| {
                ContractFailure::with_source("temporary overwrite observation failed", error).at(id)
            })?;
        verify_condition(
            matches!(observed, FixtureSupport::Supported(actual) if actual == expected),
            id,
            "temporary overwrite did not release source and publish target",
        )?;
    }
    Ok(())
}
