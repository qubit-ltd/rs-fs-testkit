// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Contract assertions for asynchronous filesystem I/O.

use qubit_fs::{
    AsyncFileSystemExt,
    FileSystemCapability,
    FsFuture,
    WriteOptions,
    WriterState,
};

use crate::AsyncFileSystemFixture;

/// Checks asynchronous write, read, commit, and abort lifecycles.
///
/// # Parameters
/// - `fixture`: Fresh asynchronous provider fixture.
///
/// # Returns
/// A future that completes after every assertion succeeds.
///
/// # Panics
/// Panics when required capabilities are absent or any lifecycle operation
/// violates the provider-neutral contract.
pub fn assert_async_write_contract(
    fixture: &dyn AsyncFileSystemFixture,
) -> FsFuture<'_, ()> {
    Box::pin(async move {
        let file_system = fixture.file_system();
        assert!(
            file_system
                .capabilities()
                .contains(FileSystemCapability::Read)
        );
        assert!(
            file_system
                .capabilities()
                .contains(FileSystemCapability::Write)
        );
        let path = fixture.path("contract-async-write.bin");
        file_system
            .write_all_async(&path, b"async contract")
            .await
            .expect("asynchronous write should commit");
        assert_eq!(
            b"async contract",
            file_system
                .read_all_async(&path, 64)
                .await
                .expect("asynchronous read should succeed")
                .as_slice(),
        );

        let abort_path = fixture.path("contract-async-abort.bin");
        let mut writer = file_system
            .open_writer_async(&abort_path, WriteOptions::default())
            .await
            .expect("asynchronous writer should open");
        writer
            .abort_async()
            .await
            .expect("open asynchronous writer should abort");
        assert_eq!(WriterState::Aborted, writer.state());
        Ok(())
    })
}
