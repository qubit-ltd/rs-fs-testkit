// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Positive contracts for synchronous temporary resources.

use qubit_fs::{
    AtomicityRequirement, CreateDirOptions, FileSystemCapability, FileSystemExt, PersistOptions,
    TempDirOptions, TempFileOptions,
};

use crate::FileSystemFixture;

/// Checks temporary-file creation, cleanup, and persistence.
///
/// # Panics
/// Panics when the provider advertises no temp-file support or violates the
/// lifecycle contract.
pub fn assert_temp_file_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    assert!(
        file_system
            .capabilities()
            .contains(FileSystemCapability::TempFile)
    );
    let parent = fixture.path("contract-temp-file-parent");
    file_system
        .create_dir(
            &parent,
            CreateDirOptions {
                recursive: true,
                exists_ok: true,
                ..CreateDirOptions::default()
            },
        )
        .expect("temporary-file parent should be created");

    let options = TempFileOptions {
        parent: Some(parent.clone()),
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
    temporary
        .resource()
        .write_all(b"temporary contract")
        .expect("temporary content should be written");
    let target = fixture.path("contract-temp-file-target.bin");
    temporary
        .persist(
            &target,
            PersistOptions {
                atomicity: AtomicityRequirement::Preferred,
                ..PersistOptions::default()
            },
        )
        .expect("temporary file should persist");
    assert_eq!(
        b"temporary contract",
        file_system
            .read_all(&target, 64)
            .expect("persisted content should be readable")
            .as_slice(),
    );
}

/// Checks temporary-directory creation, cleanup, and persistence.
///
/// # Panics
/// Panics when the provider advertises no temp-directory support or violates
/// the lifecycle contract.
pub fn assert_temp_dir_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    assert!(
        file_system
            .capabilities()
            .contains(FileSystemCapability::TempDirectory)
    );
    let parent = fixture.path("contract-temp-dir-parent");
    file_system
        .create_dir(
            &parent,
            CreateDirOptions {
                recursive: true,
                exists_ok: true,
                ..CreateDirOptions::default()
            },
        )
        .expect("temporary-directory parent should be created");
    let options = TempDirOptions {
        parent: Some(parent),
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
    temporary
        .persist(
            &target,
            PersistOptions {
                atomicity: AtomicityRequirement::Preferred,
                ..PersistOptions::default()
            },
        )
        .expect("temporary directory should persist");
    assert!(file_system.exists(&target).expect("target should exist"));
}
