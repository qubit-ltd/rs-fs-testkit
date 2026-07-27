// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use qubit_fs::{
    AchievedAtomicity, FileKind, FileMetadata, FileResource, FileSystem, FileSystemCapabilities,
    FileSystemCapability, FileSystemId, FileSystemInfo, FileSystemLimits, FileSystemProperties,
    FsError, FsErrorKind, FsOperation, FsPath, FsResult, PersistFailure, PersistOptions,
    PersistOutcome, PublicationMethod, TempDir, TempDirOptions, TempFile, TempFileOptions,
    TempResourceSession,
};
use qubit_fs_testkit::{FileSystemFixture, assert_temp_dir_contract, assert_temp_file_contract};

/// Isolated fixture for temporary-resource contract assertions.
struct TempFixture {
    file_system: TempFileSystem,
}

impl TempFixture {
    /// Creates a fixture with the specified temporary-resource capabilities.
    fn new(capabilities: FileSystemCapabilities) -> Self {
        Self {
            file_system: TempFileSystem::new(capabilities),
        }
    }
}

impl FileSystemFixture for TempFixture {
    fn file_system(&self) -> &dyn FileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> FsPath {
        FsPath::parse(&format!("/{relative}")).expect("temporary contract paths should parse")
    }
}

/// Minimal provider that supports only temporary-resource lifecycle checks.
#[derive(Clone)]
struct TempFileSystem {
    entries: Arc<Mutex<HashMap<String, FileKind>>>,
    info: Arc<FileSystemInfo>,
    capabilities: FileSystemCapabilities,
    limits: Arc<FileSystemLimits>,
    next: Arc<AtomicUsize>,
}

impl TempFileSystem {
    /// Creates an empty temporary-resource provider.
    fn new(capabilities: FileSystemCapabilities) -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            info: Arc::new(FileSystemInfo::new(
                FileSystemId::new("temp-contract")
                    .expect("the temporary fixture ID should validate"),
                "temp-contract-provider",
                qubit_fs::PathSemantics::Hierarchical,
            )),
            capabilities,
            limits: Arc::new(FileSystemLimits::unknown()),
            next: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Allocates one unique temporary provider-local path.
    fn temporary_path(&self, parent: Option<&FsPath>, prefix: &str, suffix: &str) -> FsPath {
        let parent = parent
            .map_or("/temporary", FsPath::as_str)
            .trim_end_matches('/');
        let index = self.next.fetch_add(1, Ordering::Relaxed);
        FsPath::parse(&format!("{parent}/{prefix}{index}{suffix}"))
            .expect("temporary fixture paths should parse")
    }

    /// Registers one temporary resource and returns its ordinary file resource.
    fn create_resource(&self, path: FsPath, kind: FileKind) -> FileResource {
        self.entries
            .lock()
            .expect("the temporary fixture entries should lock")
            .insert(path.as_str().to_owned(), kind);
        FileResource::new(Arc::new(self.clone()), path)
    }

    /// Builds an unsupported temporary-resource error.
    fn unsupported_temp(&self, capability: FileSystemCapability) -> FsError {
        FsError::new(
            FsErrorKind::UnsupportedCapability,
            FsOperation::CreateTemp,
            "temporary resources are not configured",
        )
        .with_provider(self.info.provider_id())
        .with_required_capability(capability)
    }
}

impl FileSystemProperties for TempFileSystem {
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

impl FileSystem for TempFileSystem {
    fn stat(&self, path: &FsPath) -> FsResult<FileMetadata> {
        self.entries
            .lock()
            .expect("the temporary fixture entries should lock")
            .get(path.as_str())
            .cloned()
            .map(FileMetadata::new)
            .ok_or_else(|| {
                FsError::new(FsErrorKind::NotFound, FsOperation::Stat, "path is absent")
                    .with_path(path.clone())
                    .with_provider(self.info.provider_id())
            })
    }

    fn create_temp_file(&self, options: TempFileOptions) -> FsResult<TempFile> {
        if !self.capabilities.contains(FileSystemCapability::TempFile) {
            return Err(self.unsupported_temp(FileSystemCapability::TempFile));
        }
        let path = self.temporary_path(options.parent.as_ref(), &options.prefix, &options.suffix);
        let resource = self.create_resource(path.clone(), FileKind::File);
        Ok(TempFile::new(
            resource,
            TempSession::new(self.clone(), path, FileKind::File),
        ))
    }

    fn create_temp_dir(&self, options: TempDirOptions) -> FsResult<TempDir> {
        if !self
            .capabilities
            .contains(FileSystemCapability::TempDirectory)
        {
            return Err(self.unsupported_temp(FileSystemCapability::TempDirectory));
        }
        let path = self.temporary_path(options.parent.as_ref(), &options.prefix, &options.suffix);
        let resource = self.create_resource(path.clone(), FileKind::Directory);
        Ok(TempDir::new(
            resource,
            TempSession::new(self.clone(), path, FileKind::Directory),
        ))
    }
}

/// Lifecycle session for one temporary fixture resource.
struct TempSession {
    file_system: TempFileSystem,
    source: FsPath,
    kind: FileKind,
}

impl TempSession {
    /// Creates a lifecycle session for one registered temporary resource.
    fn new(file_system: TempFileSystem, source: FsPath, kind: FileKind) -> Self {
        Self {
            file_system,
            source,
            kind,
        }
    }
}

impl TempResourceSession for TempSession {
    fn cleanup(&mut self) -> FsResult<()> {
        self.file_system
            .entries
            .lock()
            .expect("the temporary fixture entries should lock")
            .remove(self.source.as_str());
        Ok(())
    }

    fn keep(&mut self) -> FsResult<()> {
        Ok(())
    }

    fn persist(
        &mut self,
        target: &FsPath,
        _options: PersistOptions,
    ) -> Result<PersistOutcome, PersistFailure> {
        let mut entries = self
            .file_system
            .entries
            .lock()
            .expect("the temporary fixture entries should lock");
        entries.remove(self.source.as_str());
        entries.insert(target.as_str().to_owned(), self.kind.clone());
        let atomicity = if self
            .file_system
            .capabilities
            .contains(FileSystemCapability::AtomicTempPersist)
        {
            AchievedAtomicity::Atomic
        } else {
            AchievedAtomicity::NonAtomic
        };
        Ok(PersistOutcome::new(
            target.clone(),
            atomicity,
            PublicationMethod::AtomicRename,
        ))
    }
}

#[test]
fn test_temp_contract_assertions_are_exported() {
    let _: fn(&dyn FileSystemFixture) = assert_temp_file_contract;
    let _: fn(&dyn FileSystemFixture) = assert_temp_dir_contract;
}

/// Verifies temporary-file contracts require no unrelated filesystem capabilities.
#[test]
fn test_temp_file_contract_accepts_temp_file_only_provider() {
    assert_temp_file_contract(&TempFixture::new(
        FileSystemCapabilities::default().with(FileSystemCapability::TempFile),
    ));
}

/// Verifies temporary-directory contracts require no unrelated filesystem capabilities.
#[test]
fn test_temp_dir_contract_accepts_temp_dir_only_provider() {
    assert_temp_dir_contract(&TempFixture::new(
        FileSystemCapabilities::default().with(FileSystemCapability::TempDirectory),
    ));
}

/// Verifies atomic temporary persistence is required when it is advertised.
#[test]
fn test_temp_file_contract_checks_atomic_persistence() {
    assert_temp_file_contract(&TempFixture::new(
        FileSystemCapabilities::default()
            .with(FileSystemCapability::TempFile)
            .with(FileSystemCapability::AtomicTempPersist),
    ));
}

/// Verifies complete suites can skip unadvertised temporary-resource positives.
#[test]
fn test_temp_contracts_skip_unadvertised_capabilities() {
    let fixture = TempFixture::new(FileSystemCapabilities::default());

    assert_temp_file_contract(&fixture);
    assert_temp_dir_contract(&fixture);
}
