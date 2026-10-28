// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Verifies temporary-directory persistence and overwrite contracts.

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::AchievedAtomicity;
use qubit_fs::metadata::AtomicityRequirement;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::path::Path;
use qubit_fs::path::RelativePath;
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
/// Returns an optional evidence-gap reason after publication or expected
/// capability rejection.
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
) -> Result<Option<String>, ContractFailure> {
    let source = temporary.path().clone();
    if let Some(reason) = prepare_temp_directory_contents(suite, id, &source).await? {
        return Ok(Some(reason));
    }
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
        return Ok(None);
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
    verify_temp_directory_contents(suite, id, outcome.target()).await
}

pub(super) async fn prepare_temp_directory_contents(
    suite: &AsyncFileSystemContractSuite<'_>,
    id: ContractCheckId,
    root: &Path,
) -> Result<Option<String>, ContractFailure> {
    const DIRECT_CONTENT: &[u8] = b"direct-temp-data";
    const NESTED_CONTENT: &[u8] = b"nested-temp-data";
    let direct = root.join(&RelativePath::parse("payload").expect("constant child path"));
    let nested = root.join(&RelativePath::parse("nested/payload").expect("constant nested path"));
    for (path, content) in [(&direct, DIRECT_CONTENT), (&nested, NESTED_CONTENT)] {
        match suite
            .fixture
            .write_file_out_of_band(path, content)
            .await
            .map_err(|error| ContractFailure::with_source("temporary directory content setup failed", error).at(id))?
        {
            FixtureSupport::Supported(()) => {}
            FixtureSupport::Unsupported => {
                return Ok(Some(
                    "temporary directory content verification needs fixture.write_file_out_of_band".to_owned(),
                ));
            }
        }
        if matches!(
            suite.fixture.exists_out_of_band(path).await.map_err(|error| {
                ContractFailure::with_source("temporary directory setup existence check failed", error).at(id)
            })?,
            FixtureSupport::Supported(false)
        ) {
            verify_condition(false, id, "temporary directory child disappeared during setup")?;
        }
        match suite.fixture.read_file(path).await.map_err(|error| {
            ContractFailure::with_source("temporary directory content setup observation failed", error).at(id)
        })? {
            FixtureSupport::Supported(actual) => verify_condition(
                actual == content,
                id,
                "temporary directory fixture preparation changed content",
            )?,
            FixtureSupport::Unsupported => {
                return Ok(Some(
                    "temporary directory content verification needs fixture.read_file".to_owned(),
                ));
            }
        }
    }
    Ok(None)
}

pub(super) async fn verify_temp_directory_contents(
    suite: &AsyncFileSystemContractSuite<'_>,
    id: ContractCheckId,
    root: &Path,
) -> Result<Option<String>, ContractFailure> {
    const DIRECT_CONTENT: &[u8] = b"direct-temp-data";
    const NESTED_CONTENT: &[u8] = b"nested-temp-data";
    let direct = root.join(&RelativePath::parse("payload").expect("constant child path"));
    let nested = root.join(&RelativePath::parse("nested/payload").expect("constant nested path"));
    for (path, expected) in [(&direct, DIRECT_CONTENT), (&nested, NESTED_CONTENT)] {
        if matches!(
            suite.fixture.exists_out_of_band(path).await.map_err(|error| {
                ContractFailure::with_source("temporary directory content existence check failed", error).at(id)
            })?,
            FixtureSupport::Supported(false)
        ) {
            verify_condition(false, id, "temporary directory publication dropped a child")?;
        }
        match suite.fixture.read_file(path).await.map_err(|error| {
            ContractFailure::with_source("temporary directory content observation failed", error).at(id)
        })? {
            FixtureSupport::Supported(actual) => {
                verify_condition(
                    actual == expected,
                    id,
                    "temporary directory publication changed content",
                )?;
            }
            FixtureSupport::Unsupported => {
                return Ok(Some(
                    "temporary directory content verification needs fixture.read_file".to_owned(),
                ));
            }
        }
    }
    Ok(None)
}

/// Verifies overwrite publication using an independently prepared target.
///
/// # Parameters
///
/// * `id` - Basic temporary-directory scenario being checked.
///
/// # Returns
///
/// Returns an evidence-gap reason when required independent hooks are
/// unsupported.
///
/// # Errors
///
/// Returns a contract failure when fixture preparation, persistence, or path
/// observations differ from the overwrite contract.
pub(super) async fn verify_temp_directory_overwrite(
    suite: &mut AsyncFileSystemContractSuite<'_>,
    id: ContractCheckId,
) -> Result<Option<String>, ContractFailure> {
    let filesystem = suite.fixture.file_system();
    let relative = suite.context.relative_name("temp-overwritten-directory");
    let prepared =
        suite.fixture.seed_empty_directory(&relative).await.map_err(|error| {
            ContractFailure::with_source("temporary overwrite target preparation failed", error).at(id)
        })?;
    let target = match prepared {
        FixtureSupport::Supported(target) => target,
        FixtureSupport::Unsupported => {
            return Ok(Some(
                "temporary directory overwrite needs fixture.seed_empty_directory".to_owned(),
            ));
        }
    };
    suite.context.record_created(target.clone());
    let old_payload = target.join(&RelativePath::parse("payload").expect("constant old child path"));
    match suite
        .fixture
        .write_file_out_of_band(&old_payload, b"old-temp-data")
        .await
        .map_err(|error| ContractFailure::with_source("temporary overwrite old content setup failed", error).at(id))?
    {
        FixtureSupport::Supported(()) => {}
        FixtureSupport::Unsupported => {
            return Ok(Some(
                "temporary directory overwrite needs fixture.write_file_out_of_band".to_owned(),
            ));
        }
    }
    match suite.fixture.read_file(&old_payload).await.map_err(|error| {
        ContractFailure::with_source("temporary overwrite old content observation failed", error).at(id)
    })? {
        FixtureSupport::Supported(actual) => {
            verify_condition(
                actual == b"old-temp-data",
                id,
                "temporary overwrite fixture setup changed old content",
            )?;
        }
        FixtureSupport::Unsupported => {
            return Ok(Some("temporary directory overwrite needs fixture.read_file".to_owned()));
        }
    }
    let mut replacement = filesystem
        .create_temp_directory(TempOptions::default())
        .await
        .map_err(|error| ContractFailure::with_owned_source("temporary overwrite preparation failed", error).at(id))?;
    let source = replacement.path().clone();
    suite.context.record_created(source.clone());
    if let Some(reason) = prepare_temp_directory_contents(suite, id, &source).await? {
        return Ok(Some(reason));
    }
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
    verify_temp_directory_contents(suite, id, outcome.target()).await
}
