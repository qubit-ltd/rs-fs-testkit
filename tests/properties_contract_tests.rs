// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use std::panic::{
    AssertUnwindSafe,
    catch_unwind,
};

use qubit_fs::{
    FileMetadata,
    FileSystem,
    FileSystemCapabilities,
    FileSystemCapability,
    FileSystemId,
    FileSystemInfo,
    FileSystemLimit,
    FileSystemLimits,
    FileSystemProperties,
    FsError,
    FsErrorKind,
    FsOperation,
    FsPath,
    FsResult,
    PathSemantics,
};

use common::{
    MemoryFault,
    MemoryFixture,
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
                    FileSystemId::new("test")
                        .expect("the fixture ID should validate"),
                    "test-provider",
                    PathSemantics::Hierarchical,
                ),
                capabilities,
                limits: FileSystemLimits::unknown(),
            },
        }
    }

    /// Creates a fixture with explicit properties and limits.
    fn with_limits(limits: FileSystemLimits) -> Self {
        let mut fixture = Self::new(FileSystemCapabilities::default());
        fixture.file_system.limits = limits;
        fixture
    }
}

impl qubit_fs_testkit::FileSystemFixture for PropertiesFixture {
    fn file_system(&self) -> &dyn FileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> FsPath {
        FsPath::parse(&format!("/{relative}"))
            .expect("contract fixture paths should parse")
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

/// Verifies every derived capability requires its base operation.
#[test]
fn test_capabilities_contract_rejects_every_missing_base_capability() {
    let dependencies = [
        FileSystemCapability::RangeRead,
        FileSystemCapability::ConditionalRead,
        FileSystemCapability::ChecksumValidation,
        FileSystemCapability::Append,
        FileSystemCapability::ConditionalWrite,
        FileSystemCapability::AtomicReplace,
        FileSystemCapability::RecursiveDelete,
        FileSystemCapability::ConditionalDelete,
        FileSystemCapability::AtomicRename,
        FileSystemCapability::ServerSideCopy,
    ];

    for derived in dependencies {
        let fixture = PropertiesFixture::new(
            FileSystemCapabilities::default().with(derived),
        );
        let result = catch_unwind(AssertUnwindSafe(|| {
            qubit_fs_testkit::assert_capabilities_contract(&fixture);
        }));
        assert!(
            result.is_err(),
            "{derived:?} without its base capability must be rejected",
        );
    }
}

/// Verifies the properties contract rejects unstable capability snapshots.
#[test]
#[should_panic(expected = "filesystem capabilities must be stable")]
fn test_properties_contract_rejects_unstable_capabilities() {
    let fixture = MemoryFixture::with_fault(MemoryFault::UnstableCapabilities);

    qubit_fs_testkit::assert_properties_contract(&fixture);
}

/// Verifies property contracts reject providers that ignore finite component
/// limits.
#[test]
#[should_panic(expected = "filesystem error kind must match")]
fn test_properties_contract_rejects_unenforced_finite_component_limit() {
    let limits = FileSystemLimits::unknown()
        .with_max_component_text_bytes(FileSystemLimit::Maximum(64));

    qubit_fs_testkit::assert_properties_contract(
        &PropertiesFixture::with_limits(limits),
    );
}

/// Verifies property contracts accept providers that enforce finite component
/// limits.
#[test]
fn test_properties_contract_accepts_enforced_finite_component_limit() {
    let limits = FileSystemLimits::unknown()
        .with_max_component_text_bytes(FileSystemLimit::Maximum(64));

    qubit_fs_testkit::assert_properties_contract(
        &MemoryFixture::with_capabilities_and_limits(
            FileSystemCapabilities::default(),
            limits,
        ),
    );
}

/// Verifies property contracts exercise finite path and write limits safely.
#[test]
fn test_properties_contract_accepts_enforced_finite_path_and_write_limits() {
    let limits = FileSystemLimits::unknown()
        .with_max_path_text_bytes(FileSystemLimit::Maximum(64))
        .with_max_write_bytes(FileSystemLimit::Maximum(64));

    qubit_fs_testkit::assert_properties_contract(
        &MemoryFixture::with_capabilities_and_limits(
            FileSystemCapabilities::default().with(FileSystemCapability::Write),
            limits,
        ),
    );
}
