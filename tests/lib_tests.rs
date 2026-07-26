// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_fs::{
    FileMetadata, FileSystem, FileSystemCapabilities, FileSystemCapability, FileSystemId,
    FileSystemInfo, FileSystemLimits, FileSystemProperties, FsError, FsErrorKind, FsOperation,
    FsPath, FsResult, PathSemantics,
};

struct PropertiesFileSystem {
    info: FileSystemInfo,
    capabilities: FileSystemCapabilities,
    limits: FileSystemLimits,
}

impl FileSystemProperties for PropertiesFileSystem {
    fn info(&self) -> &FileSystemInfo {
        &self.info
    }

    fn capabilities(&self) -> FileSystemCapabilities {
        self.capabilities
    }

    fn limits(&self) -> &FileSystemLimits {
        &self.limits
    }
}

impl FileSystem for PropertiesFileSystem {
    fn stat(&self, path: &FsPath) -> FsResult<FileMetadata> {
        Err(FsError::new(
            FsErrorKind::NotFound,
            FsOperation::Stat,
            "the properties fixture contains no files",
        )
        .with_path(path.clone())
        .with_provider(self.info.provider_id()))
    }
}

struct PropertiesFixture {
    file_system: PropertiesFileSystem,
}

impl PropertiesFixture {
    /// Creates a fixture with the specified capability set.
    fn new(capabilities: FileSystemCapabilities) -> Self {
        Self {
            file_system: PropertiesFileSystem {
                info: FileSystemInfo::new(
                    FileSystemId::new("test").expect("the fixture ID should validate"),
                    "test-provider",
                    PathSemantics::Hierarchical,
                ),
                capabilities,
                limits: FileSystemLimits::unknown(),
            },
        }
    }
}

impl qubit_fs_testkit::FileSystemFixture for PropertiesFixture {
    fn file_system(&self) -> &dyn FileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> FsPath {
        FsPath::parse(&format!("/{relative}")).expect("contract fixture paths should parse")
    }
}

/// Verifies the reusable properties contract accepts stable valid properties.
#[test]
fn test_properties_contract_accepts_stable_properties() {
    let fixture = PropertiesFixture::new(
        FileSystemCapabilities::default()
            .with(FileSystemCapability::Read)
            .with(FileSystemCapability::Write),
    );

    qubit_fs_testkit::assert_properties_contract(&fixture);
}

/// Verifies the reusable capability contract accepts consistent capabilities.
#[test]
fn test_capabilities_contract_accepts_consistent_capabilities() {
    let fixture = PropertiesFixture::new(
        FileSystemCapabilities::default()
            .with(FileSystemCapability::Read)
            .with(FileSystemCapability::Write)
            .with(FileSystemCapability::Append),
    );

    qubit_fs_testkit::assert_capabilities_contract(&fixture);
}

/// Verifies capability dependencies reject append without write support.
#[test]
#[should_panic(expected = "Append requires Write")]
fn test_capabilities_contract_rejects_append_without_write() {
    let fixture = PropertiesFixture::new(
        FileSystemCapabilities::default().with(FileSystemCapability::Append),
    );

    qubit_fs_testkit::assert_capabilities_contract(&fixture);
}
