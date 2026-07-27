// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Positive contracts for synchronous temporary resources.

use qubit_fs::{
    AchievedAtomicity, AtomicityRequirement, FileSystemCapability, FsErrorKind, FsOperation,
    PersistFailureState, PersistOptions, TempDirOptions, TempFileOptions,
};

use crate::{FileSystemFixture, internal::assert_error};

/// Checks temporary-file creation, cleanup, and persistence.
///
/// # Panics
/// Returns without a positive assertion when `TempFile` is unadvertised;
/// [`crate::assert_unsupported_operations_contract`] checks that negative path.
/// Panics when an advertised provider violates the lifecycle contract.
pub fn assert_temp_file_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    if !file_system
        .capabilities()
        .contains(FileSystemCapability::TempFile)
    {
        return;
    }
    let options = TempFileOptions {
        parent: None,
        prefix: "contract-".to_owned(),
        suffix: ".tmp".to_owned(),
    };
    let mut temporary = file_system
        .create_temp_file(options.clone())
        .expect("temporary file should be created");
    assert!(
        file_system
            .exists(temporary.path())
            .expect("temp should exist")
    );
    temporary.cleanup().expect("temporary file should clean up");

    let mut temporary = file_system
        .create_temp_file(options)
        .expect("temporary file should be recreated");
    let target = fixture.path("contract-temp-file-target.bin");
    let outcome = temporary
        .persist(
            &target,
            PersistOptions {
                atomicity: temp_persist_atomicity(file_system),
                ..PersistOptions::default()
            },
        )
        .expect("temporary file should persist");
    assert_persist_outcome(file_system, &outcome, &target);
    assert_required_atomic_file_persist_rejection(fixture);
}

/// Checks temporary-directory creation, cleanup, and persistence.
///
/// # Panics
/// Returns without a positive assertion when `TempDirectory` is unadvertised;
/// [`crate::assert_unsupported_operations_contract`] checks that negative path.
/// Panics when an advertised provider violates the lifecycle contract.
pub fn assert_temp_dir_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    if !file_system
        .capabilities()
        .contains(FileSystemCapability::TempDirectory)
    {
        return;
    }
    let options = TempDirOptions {
        parent: None,
        prefix: "contract-dir-".to_owned(),
        suffix: ".tmp".to_owned(),
    };
    let mut temporary = file_system
        .create_temp_dir(options.clone())
        .expect("temporary directory should be created");
    assert!(
        file_system
            .exists(temporary.path())
            .expect("temp should exist")
    );
    temporary
        .cleanup()
        .expect("temporary directory should clean up");

    let mut temporary = file_system
        .create_temp_dir(options)
        .expect("temporary directory should be recreated");
    let target = fixture.path("contract-temp-dir-target");
    let outcome = temporary
        .persist(
            &target,
            PersistOptions {
                atomicity: temp_persist_atomicity(file_system),
                ..PersistOptions::default()
            },
        )
        .expect("temporary directory should persist");
    assert_persist_outcome(file_system, &outcome, &target);
    assert_required_atomic_dir_persist_rejection(fixture);
}

/// Selects the strongest persistence atomicity that the provider advertises.
///
/// # Parameters
///
/// * `file_system` - Filesystem whose temporary-persistence guarantees apply.
///
/// # Returns
/// `Required` when atomic temporary persistence is advertised; otherwise
/// `Preferred`.
#[must_use]
fn temp_persist_atomicity(file_system: &dyn qubit_fs::FileSystem) -> AtomicityRequirement {
    if file_system
        .capabilities()
        .contains(FileSystemCapability::AtomicTempPersist)
    {
        AtomicityRequirement::Required
    } else {
        AtomicityRequirement::Preferred
    }
}

/// Checks a successful temporary persistence outcome and its destination.
///
/// # Parameters
///
/// * `file_system` - Filesystem that published the temporary resource.
/// * `outcome` - Provider-reported successful persistence outcome.
/// * `target` - Requested final provider-local path.
///
/// # Panics
///
/// Panics when the reported target differs, the destination is absent, or an
/// advertised atomic guarantee is not achieved.
#[track_caller]
fn assert_persist_outcome(
    file_system: &dyn qubit_fs::FileSystem,
    outcome: &qubit_fs::PersistOutcome,
    target: &qubit_fs::FsPath,
) {
    assert_eq!(
        target, &outcome.target,
        "persist must report the requested target"
    );
    assert!(
        file_system
            .exists(target)
            .expect("persisted target should be statable"),
        "persist must create the requested target",
    );
    if file_system
        .capabilities()
        .contains(FileSystemCapability::AtomicTempPersist)
    {
        assert_eq!(
            AchievedAtomicity::Atomic,
            outcome.atomicity,
            "advertised atomic temporary persistence must report atomic publication",
        );
    }
}

/// Checks that an unadvertised atomic temporary-persistence request fails early.
///
/// # Parameters
///
/// * `fixture` - Fixture supplying a fresh temporary file.
///
/// # Panics
///
/// Panics when an unadvertised atomic persistence request succeeds or returns
/// an incorrectly structured failure.
#[track_caller]
fn assert_required_atomic_file_persist_rejection(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    if file_system
        .capabilities()
        .contains(FileSystemCapability::AtomicTempPersist)
    {
        return;
    }
    let target = fixture.path("contract-temp-required-atomic-target");
    let mut temporary = file_system
        .create_temp_file(TempFileOptions::default())
        .expect("temporary file should be created for atomic preflight");
    let source = temporary.path().clone();
    let failure = temporary
        .persist(&target, PersistOptions::default())
        .expect_err("unadvertised atomic temporary persistence must reject before publication");
    assert_eq!(
        PersistFailureState::NotPublished,
        failure.state(),
        "missing atomic persistence must not publish the target",
    );
    assert_error(
        failure.error(),
        FsErrorKind::RequirementNotMet,
        FsOperation::PersistTemp,
        Some(&source),
        None,
        Some(FileSystemCapability::AtomicTempPersist),
    );
}

/// Checks that an unadvertised atomic temporary-directory persistence request fails early.
///
/// # Parameters
///
/// * `fixture` - Fixture supplying a fresh temporary directory.
///
/// # Panics
///
/// Panics when an unadvertised atomic persistence request succeeds or returns
/// an incorrectly structured failure.
#[track_caller]
fn assert_required_atomic_dir_persist_rejection(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    if file_system
        .capabilities()
        .contains(FileSystemCapability::AtomicTempPersist)
    {
        return;
    }
    let target = fixture.path("contract-temp-dir-required-atomic-target");
    let mut temporary = file_system
        .create_temp_dir(TempDirOptions::default())
        .expect("temporary directory should be created for atomic preflight");
    let source = temporary.path().clone();
    let failure = temporary
        .persist(&target, PersistOptions::default())
        .expect_err("unadvertised atomic temporary persistence must reject before publication");
    assert_eq!(
        PersistFailureState::NotPublished,
        failure.state(),
        "missing atomic persistence must not publish the target",
    );
    assert_error(
        failure.error(),
        FsErrorKind::RequirementNotMet,
        FsOperation::PersistTemp,
        Some(&source),
        None,
        Some(FileSystemCapability::AtomicTempPersist),
    );
}
