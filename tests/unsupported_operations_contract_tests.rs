// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use qubit_fs::{
    FileMetadata, FileReader, FileSystem, FileSystemCapabilities, FileSystemCapability,
    FileSystemId, FileSystemInfo, FileSystemLimits, FileSystemProperties, FsError, FsErrorKind,
    FsOperation, FsPath, FsResult, PathSemantics, ReadOptions,
};
use qubit_fs_testkit::FileSystemFixture;

use common::MemoryFixture;

struct DefaultUnsupportedFileSystem {
    info: FileSystemInfo,
    limits: FileSystemLimits,
    read_fault: Option<ReadFault>,
}

#[derive(Clone, Copy)]
enum ReadFault {
    WrongOperation,
    MissingPath,
    WrongCapability,
}

impl DefaultUnsupportedFileSystem {
    /// Creates a filesystem that relies on every optional-operation default.
    fn new(read_fault: Option<ReadFault>) -> Self {
        Self {
            info: FileSystemInfo::new(
                FileSystemId::new("default-unsupported").expect("the fixture ID should validate"),
                "default-unsupported-provider",
                PathSemantics::Hierarchical,
            ),
            limits: FileSystemLimits::unknown(),
            read_fault,
        }
    }
}

impl FileSystemProperties for DefaultUnsupportedFileSystem {
    fn info(&self) -> &FileSystemInfo {
        &self.info
    }

    fn capabilities(&self) -> FileSystemCapabilities {
        FileSystemCapabilities::default()
    }

    fn limits(&self) -> &FileSystemLimits {
        &self.limits
    }
}

impl FileSystem for DefaultUnsupportedFileSystem {
    fn stat(&self, path: &FsPath) -> FsResult<FileMetadata> {
        Err(FsError::new(
            FsErrorKind::NotFound,
            FsOperation::Stat,
            "the default unsupported fixture contains no files",
        )
        .with_path(path.clone())
        .with_provider(self.info.provider_id()))
    }

    fn open_reader(&self, path: &FsPath, _options: ReadOptions) -> FsResult<FileReader> {
        let Some(fault) = self.read_fault else {
            return Err(FsError::new(
                FsErrorKind::UnsupportedCapability,
                FsOperation::OpenReader,
                "filesystem capability is not supported",
            )
            .with_path(path.clone())
            .with_required_capability(FileSystemCapability::Read));
        };
        let (operation, error_path, capability) = match fault {
            ReadFault::WrongOperation => (
                FsOperation::List,
                Some(path.clone()),
                FileSystemCapability::Read,
            ),
            ReadFault::MissingPath => (FsOperation::OpenReader, None, FileSystemCapability::Read),
            ReadFault::WrongCapability => (
                FsOperation::OpenReader,
                Some(path.clone()),
                FileSystemCapability::Write,
            ),
        };
        let mut error = FsError::new(
            FsErrorKind::UnsupportedCapability,
            operation,
            "the fixture returns a malformed unsupported-operation error",
        )
        .with_required_capability(capability);
        if let Some(error_path) = error_path {
            error = error.with_path(error_path);
        }
        Err(error)
    }
}

struct DefaultUnsupportedFixture {
    file_system: DefaultUnsupportedFileSystem,
}

impl DefaultUnsupportedFixture {
    /// Creates a fixture with optional read-error corruption.
    fn new(read_fault: Option<ReadFault>) -> Self {
        Self {
            file_system: DefaultUnsupportedFileSystem::new(read_fault),
        }
    }
}

impl FileSystemFixture for DefaultUnsupportedFixture {
    fn file_system(&self) -> &dyn FileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> FsPath {
        FsPath::parse(&format!("/{relative}")).expect("contract fixture paths should parse")
    }
}

/// Verifies unsupported operations expose structured capability errors.
#[test]
fn test_unsupported_operations_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_unsupported_operations_contract(&DefaultUnsupportedFixture::new(None));
}

/// Verifies malformed operation fields are rejected for unsupported reads.
#[test]
#[should_panic(expected = "filesystem error operation must match")]
fn test_unsupported_operations_contract_rejects_wrong_operation() {
    qubit_fs_testkit::assert_unsupported_operations_contract(&DefaultUnsupportedFixture::new(
        Some(ReadFault::WrongOperation),
    ));
}

/// Verifies malformed path fields are rejected for unsupported reads.
#[test]
#[should_panic(expected = "filesystem error path must match")]
fn test_unsupported_operations_contract_rejects_missing_path() {
    qubit_fs_testkit::assert_unsupported_operations_contract(&DefaultUnsupportedFixture::new(
        Some(ReadFault::MissingPath),
    ));
}

/// Verifies malformed capability fields are rejected for unsupported reads.
#[test]
#[should_panic(expected = "filesystem error required capability must match")]
fn test_unsupported_operations_contract_rejects_wrong_capability() {
    qubit_fs_testkit::assert_unsupported_operations_contract(&DefaultUnsupportedFixture::new(
        Some(ReadFault::WrongCapability),
    ));
}

/// Verifies the unsupported-operation contract also checks reader support.
#[test]
#[should_panic(expected = "filesystem error kind must match")]
fn test_unsupported_operations_contract_rejects_incorrect_read_error() {
    let fixture = MemoryFixture::with_capabilities(FileSystemCapabilities::default());

    qubit_fs_testkit::assert_unsupported_operations_contract(&fixture);
}

/// Verifies the unsupported-operation contract also checks writer support.
#[test]
#[should_panic(expected = "unadvertised write must fail")]
fn test_unsupported_operations_contract_rejects_incorrect_write_error() {
    let capabilities = FileSystemCapabilities::default().with(FileSystemCapability::Read);
    let fixture = MemoryFixture::with_capabilities(capabilities);

    qubit_fs_testkit::assert_unsupported_operations_contract(&fixture);
}
