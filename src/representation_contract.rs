// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Contract assertions for provider-specific resource representations.

use qubit_fs::{FileKind, FileSystemCapability};

use crate::{FileSystemFixture, io_contract::require_capability};

/// Checks that an advertised empty directory or prefix remains representable.
///
/// # Parameters
///
/// * `fixture` - Fixture that supplies an existing empty directory or prefix.
///
/// # Panics
///
/// Panics when `EmptyDirectory` is not advertised, the fixture cannot prepare
/// a probe resource, or stat does not report a directory-like kind.
#[track_caller]
pub fn assert_empty_directory_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    require_capability(file_system, FileSystemCapability::EmptyDirectory);
    let path = fixture
        .empty_directory_path()
        .expect("EmptyDirectory contract requires fixture.empty_directory_path()");
    let metadata = file_system
        .stat(&path)
        .expect("stat must read the empty directory or prefix");
    assert!(
        metadata.is_directory_like(),
        "empty directory capability must report a directory-like kind",
    );
}

/// Checks that an advertised symbolic link remains visible to stat.
///
/// # Parameters
///
/// * `fixture` - Fixture that supplies an existing symbolic link.
///
/// # Panics
///
/// Panics when `Symlink` is not advertised, the fixture cannot prepare a probe
/// link, or stat follows the final link instead of reporting `Symlink`.
#[track_caller]
pub fn assert_symlink_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    require_capability(file_system, FileSystemCapability::Symlink);
    let path = fixture
        .symlink_path()
        .expect("Symlink contract requires fixture.symlink_path()");
    let metadata = file_system
        .stat(&path)
        .expect("stat must read the symbolic-link metadata");
    assert_eq!(
        FileKind::Symlink,
        metadata.kind,
        "stat must not dereference the final symbolic link",
    );
}
