// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Contract assertions for operations a provider does not advertise.

use qubit_fs::{
    CopyOptions, CreateDirOptions, DeleteOptions, FileSystemCapability, FsErrorKind, FsOperation,
    ListOptions, RenameOptions, TempDirOptions, TempFileOptions,
};

use crate::{FileSystemFixture, internal::assert_error};

/// Checks structured errors for every unadvertised synchronous operation.
///
/// Advertised operations are skipped because their positive behavior requires
/// operation-specific fixtures beyond the current synchronous read/write
/// contract.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture whose unsupported operations are
///   checked.
///
/// # Panics
///
/// Panics when an unadvertised operation succeeds or returns an error with an
/// incorrect kind, operation, path, or required capability.
pub fn assert_unsupported_operations_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    let source = fixture.path("contract-unsupported-source.bin");
    let destination = fixture.path("contract-unsupported-destination.bin");

    if !file_system
        .capabilities()
        .contains(FileSystemCapability::List)
    {
        let error = file_system
            .list(&source, ListOptions::default())
            .expect_err("unadvertised list must fail");
        assert_error(
            &error,
            FsErrorKind::UnsupportedCapability,
            FsOperation::List,
            Some(&source),
            None,
            Some(FileSystemCapability::List),
        );
    }

    if !file_system
        .capabilities()
        .contains(FileSystemCapability::CreateDirectory)
    {
        let error = file_system
            .create_dir(&source, CreateDirOptions::default())
            .expect_err("unadvertised directory creation must fail");
        assert_error(
            &error,
            FsErrorKind::UnsupportedCapability,
            FsOperation::CreateDir,
            Some(&source),
            None,
            Some(FileSystemCapability::CreateDirectory),
        );
    }

    if !file_system
        .capabilities()
        .contains(FileSystemCapability::Delete)
    {
        let error = file_system
            .delete(&source, DeleteOptions::default())
            .expect_err("unadvertised deletion must fail");
        assert_error(
            &error,
            FsErrorKind::UnsupportedCapability,
            FsOperation::Delete,
            Some(&source),
            None,
            Some(FileSystemCapability::Delete),
        );
    }

    if !file_system
        .capabilities()
        .contains(FileSystemCapability::Rename)
    {
        let error = file_system
            .rename(&source, &destination, RenameOptions::default())
            .expect_err("unadvertised rename must fail");
        assert_error(
            &error,
            FsErrorKind::UnsupportedCapability,
            FsOperation::Rename,
            Some(&source),
            None,
            Some(FileSystemCapability::Rename),
        );
    }

    if !file_system
        .capabilities()
        .contains(FileSystemCapability::Copy)
    {
        let error = file_system
            .copy(&source, &destination, CopyOptions::default())
            .expect_err("unadvertised copy must fail");
        assert_error(
            &error,
            FsErrorKind::UnsupportedCapability,
            FsOperation::Copy,
            Some(&source),
            None,
            Some(FileSystemCapability::Copy),
        );
    }

    if !file_system
        .capabilities()
        .contains(FileSystemCapability::TempFile)
    {
        let error = file_system
            .create_temp_file(TempFileOptions::default())
            .expect_err("unadvertised temporary-file creation must fail");
        assert_error(
            &error,
            FsErrorKind::UnsupportedCapability,
            FsOperation::CreateTemp,
            None,
            None,
            Some(FileSystemCapability::TempFile),
        );
    }

    if !file_system
        .capabilities()
        .contains(FileSystemCapability::TempDirectory)
    {
        let error = file_system
            .create_temp_dir(TempDirOptions::default())
            .expect_err("unadvertised temporary-directory creation must fail");
        assert_error(
            &error,
            FsErrorKind::UnsupportedCapability,
            FsOperation::CreateTemp,
            None,
            None,
            Some(FileSystemCapability::TempDirectory),
        );
    }
}
