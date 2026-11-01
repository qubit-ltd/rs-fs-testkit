// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Verifies temporary-directory keep and cleanup lifecycle contracts.

use qubit_fs::path::RelativePath;
use qubit_fs::temp::AsyncTempDirectory;
use qubit_fs::temp::TempOptions;
use qubit_fs::temp::TempResourceState;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractFailure;
use crate::ContractTempFailure;
use crate::FixtureSupport;
use crate::internal::verify_condition;

mod internal;

/// Checks keep, repeated keep, and repeated cleanup before persistence.
///
/// Atomic persistence scenarios bypass keep. The repeated-lifecycle scenario
/// completes after verifying both kept and cleaned resources reject reuse.
///
/// # Parameters
///
/// * `suite` - Contract suite providing the filesystem and observation hooks.
/// * `id` - Temporary-directory scenario selecting the lifecycle contract.
/// * `temporary` - Owned resource created for that scenario.
///
/// # Returns
///
/// Returns the resource for persistence, or `None` when lifecycle checks
/// finish.
///
/// # Errors
///
/// Returns a contract failure when a lifecycle operation or observation
/// differs.
pub(super) async fn prepare_temp_directory_persistence(
    suite: &mut AsyncFileSystemContractSuite<'_>,
    id: ContractCheckId,
    mut temporary: AsyncTempDirectory,
) -> Result<(Option<AsyncTempDirectory>, Option<String>), ContractFailure> {
    let filesystem = suite.fixture.file_system();
    let source = temporary.path().clone();
    if let Some(reason) = super::persistence::prepare_temp_directory_contents(suite, id, &source).await? {
        return Ok((None, Some(reason)));
    }
    if id == ContractCheckId::TempAtomic {
        return Ok((Some(temporary), None));
    }
    let kept_source = source;
    let kept = match temporary.keep().await {
        Ok(outcome) => outcome,
        Err(error) => {
            return Err(ContractFailure::with_owned_source(
                "temporary keep failed",
                ContractTempFailure::new(error, temporary),
            )
            .at(id));
        }
    };
    suite.context.record_created(kept.target().clone());
    verify_condition(
        temporary.state() == TempResourceState::Kept && kept.target() != &kept_source,
        id,
        "temporary keep state or publication identity differs",
    )?;
    for (path, expected) in [(&kept_source, false), (kept.target(), true)] {
        let observed = suite
            .fixture
            .exists_out_of_band(path)
            .await
            .map_err(|error| ContractFailure::with_source("temporary keep observation failed", error).at(id))?;
        verify_condition(
            matches!(observed, FixtureSupport::Supported(actual) if actual == expected),
            id,
            "temporary keep publication differs",
        )?;
    }
    if let Some(reason) = super::persistence::verify_temp_directory_contents(suite, id, kept.target()).await? {
        return Ok((None, Some(reason)));
    }
    if id == ContractCheckId::TempRepeatedLifecycle {
        let reason = internal::verify_repeated_temp_directory_lifecycle(suite, id, temporary, kept.target()).await?;
        if reason.is_some() {
            return Ok((None, reason));
        }
        let mut first = filesystem
            .create_temp_directory(TempOptions::default())
            .await
            .map_err(|error| {
                ContractFailure::with_owned_source("interleaved temporary preparation failed", error).at(id)
            })?;
        suite.context.record_created(first.path().clone());
        let mut second = filesystem
            .create_temp_directory(TempOptions::default())
            .await
            .map_err(|error| {
                ContractFailure::with_owned_source("interleaved temporary preparation failed", error).at(id)
            })?;
        let second_path = second.path().clone();
        suite.context.record_created(second_path.clone());
        let sentinel =
            second_path.join(&RelativePath::parse("nested/sentinel").expect("constant temporary sentinel path"));
        match suite
            .fixture
            .write_file_out_of_band(&sentinel, b"directory B sentinel")
            .await
            .map_err(|error| {
                ContractFailure::with_source("interleaved temporary sentinel setup failed", error).at(id)
            })? {
            FixtureSupport::Supported(()) => {}
            FixtureSupport::Unsupported => {
                return Ok((
                    None,
                    Some("temp/repeated-lifecycle needs fixture.write_file_out_of_band".to_owned()),
                ));
            }
        }
        if let Err(error) = first.cleanup().await {
            return Err(ContractFailure::with_owned_source(
                "interleaved temporary cleanup failed",
                ContractTempFailure::new(error, first),
            )
            .at(id));
        }
        for path in [&second_path, &sentinel] {
            let observed = suite.fixture.exists_out_of_band(path).await.map_err(|error| {
                ContractFailure::with_source("interleaved temporary observation failed", error).at(id)
            })?;
            match observed {
                FixtureSupport::Supported(true) => {}
                FixtureSupport::Supported(false) => verify_condition(
                    false,
                    id,
                    "temp/repeated-lifecycle: live directory or sentinel disappeared after cleanup",
                )?,
                FixtureSupport::Unsupported => {
                    return Ok((
                        None,
                        Some(
                            "temp/repeated-lifecycle needs fixture.exists_out_of_band for live directory evidence"
                                .to_owned(),
                        ),
                    ));
                }
            }
        }
        match suite.fixture.read_file(&sentinel).await.map_err(|error| {
            ContractFailure::with_source("interleaved temporary sentinel observation failed", error).at(id)
        })? {
            FixtureSupport::Supported(actual) => verify_condition(
                actual.as_slice() == b"directory B sentinel",
                id,
                "temp/repeated-lifecycle: live directory sentinel changed after cleanup",
            )?,
            FixtureSupport::Unsupported => {
                return Ok((None, Some("temp/repeated-lifecycle needs fixture.read_file".to_owned())));
            }
        }
        let mut third = filesystem
            .create_temp_directory(TempOptions::default())
            .await
            .map_err(|error| {
                ContractFailure::with_owned_source("interleaved temporary preparation failed", error).at(id)
            })?;
        suite.context.record_created(third.path().clone());
        verify_condition(
            third.path() != &second_path,
            id,
            "temp/repeated-lifecycle: live directory path was reused",
        )?;
        match suite.fixture.read_file(&sentinel).await.map_err(|error| {
            ContractFailure::with_source("interleaved temporary sentinel observation failed", error).at(id)
        })? {
            FixtureSupport::Supported(actual) => verify_condition(
                actual.as_slice() == b"directory B sentinel",
                id,
                "temp/repeated-lifecycle: live directory sentinel changed after creating another resource",
            )?,
            FixtureSupport::Unsupported => {
                return Ok((None, Some("temp/repeated-lifecycle needs fixture.read_file".to_owned())));
            }
        }
        if let Err(error) = second.cleanup().await {
            return Err(ContractFailure::with_owned_source(
                "interleaved temporary cleanup failed",
                ContractTempFailure::new(error, second),
            )
            .at(id));
        }
        if let Err(error) = third.cleanup().await {
            return Err(ContractFailure::with_owned_source(
                "interleaved temporary cleanup failed",
                ContractTempFailure::new(error, third),
            )
            .at(id));
        }
        return Ok((None, None));
    }
    let temporary = filesystem
        .create_temp_directory(TempOptions::default())
        .await
        .map_err(|error| {
            ContractFailure::with_owned_source("temporary persistence preparation failed", error).at(id)
        })?;
    suite.context.record_created(temporary.path().clone());
    Ok((Some(temporary), None))
}
