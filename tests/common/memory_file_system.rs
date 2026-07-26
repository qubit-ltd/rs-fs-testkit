// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::{
    collections::HashMap,
    io::{Cursor, Write},
    sync::{Arc, Mutex},
};

use qubit_fs::{
    AchievedAtomicity, AtomicityRequirement, FileKind, FileLocation, FileMetadata, FileReader,
    FileSystem, FileSystemCapabilities, FileSystemCapability, FileSystemId, FileSystemInfo,
    FileSystemLimits, FileSystemProperties, FileWriteSession, FileWriter, FsError, FsErrorKind,
    FsOperation, FsPath, FsResult, OpenedFileInfo, PathSemantics, PublicationMethod, ReadOptions,
    WriteDisposition, WriteFailure, WriteOptions, WriteOutcome,
};
use qubit_fs_testkit::FileSystemFixture;

type Files = Arc<Mutex<HashMap<String, Vec<u8>>>>;

pub struct MemoryFixture {
    file_system: MemoryFileSystem,
}

impl MemoryFixture {
    /// Creates an isolated fully capable in-memory filesystem.
    pub fn new() -> Self {
        Self::with_capabilities(
            FileSystemCapabilities::default()
                .with(FileSystemCapability::Read)
                .with(FileSystemCapability::Write)
                .with(FileSystemCapability::Append)
                .with(FileSystemCapability::AtomicReplace),
        )
    }

    /// Creates an isolated filesystem with the specified capabilities.
    pub fn with_capabilities(capabilities: FileSystemCapabilities) -> Self {
        Self {
            file_system: MemoryFileSystem::new(capabilities),
        }
    }
}

impl FileSystemFixture for MemoryFixture {
    fn file_system(&self) -> &dyn FileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> FsPath {
        FsPath::parse(&format!("/{relative}")).expect("contract fixture paths should parse")
    }
}

struct MemoryFileSystem {
    files: Files,
    info: FileSystemInfo,
    capabilities: FileSystemCapabilities,
    limits: FileSystemLimits,
}

impl MemoryFileSystem {
    /// Creates an empty filesystem with the specified capabilities.
    fn new(capabilities: FileSystemCapabilities) -> Self {
        Self {
            files: Arc::new(Mutex::new(HashMap::new())),
            info: FileSystemInfo::new(
                FileSystemId::new("memory").expect("the memory filesystem ID should validate"),
                "memory-provider",
                PathSemantics::Hierarchical,
            ),
            capabilities,
            limits: FileSystemLimits::unknown(),
        }
    }

    /// Creates opened-file metadata for one path.
    fn opened_info(&self, path: &FsPath) -> OpenedFileInfo {
        OpenedFileInfo::new(FileLocation::new(self.info.id().clone(), path.clone()))
    }

    /// Adds standard provider context to an option-validation error.
    fn with_context(&self, error: FsError, path: &FsPath) -> FsError {
        error
            .with_path(path.clone())
            .with_provider(self.info.provider_id())
    }
}

impl FileSystemProperties for MemoryFileSystem {
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

impl FileSystem for MemoryFileSystem {
    fn stat(&self, path: &FsPath) -> FsResult<FileMetadata> {
        let files = self.files.lock().expect("the memory store should lock");
        match files.get(path.as_str()) {
            Some(bytes) => {
                let mut metadata = FileMetadata::new(FileKind::File);
                metadata.len = Some(bytes.len() as u64);
                Ok(metadata)
            }
            None => Err(FsError::new(
                FsErrorKind::NotFound,
                FsOperation::Stat,
                "the memory path does not exist",
            )
            .with_path(path.clone())
            .with_provider(self.info.provider_id())),
        }
    }

    fn open_reader(&self, path: &FsPath, options: ReadOptions) -> FsResult<FileReader> {
        options
            .validate_against(self.capabilities)
            .map_err(|error| self.with_context(error, path))?;
        let files = self.files.lock().expect("the memory store should lock");
        let bytes = files.get(path.as_str()).cloned().ok_or_else(|| {
            FsError::new(
                FsErrorKind::NotFound,
                FsOperation::OpenReader,
                "the memory path does not exist",
            )
            .with_path(path.clone())
            .with_provider(self.info.provider_id())
        })?;
        Ok(FileReader::new(Cursor::new(bytes), self.opened_info(path)))
    }

    fn open_writer(&self, path: &FsPath, options: WriteOptions) -> FsResult<FileWriter> {
        options
            .validate_against(self.capabilities)
            .map_err(|error| self.with_context(error, path))?;
        if options.disposition == WriteDisposition::CreateNew
            && self
                .files
                .lock()
                .expect("the memory store should lock")
                .contains_key(path.as_str())
        {
            return Err(FsError::new(
                FsErrorKind::AlreadyExists,
                FsOperation::OpenWriter,
                "the memory path already exists",
            )
            .with_path(path.clone())
            .with_provider(self.info.provider_id()));
        }
        let session = MemoryWriteSession {
            files: Arc::clone(&self.files),
            path: path.as_str().to_owned(),
            disposition: options.disposition,
            atomicity: options.atomicity,
            bytes: Vec::new(),
        };
        Ok(FileWriter::new(session, self.opened_info(path)))
    }
}

struct MemoryWriteSession {
    files: Files,
    path: String,
    disposition: WriteDisposition,
    atomicity: AtomicityRequirement,
    bytes: Vec<u8>,
}

impl Write for MemoryWriteSession {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl FileWriteSession for MemoryWriteSession {
    fn commit(&mut self) -> Result<WriteOutcome, WriteFailure> {
        let mut files = self.files.lock().expect("the memory store should lock");
        match self.disposition {
            WriteDisposition::Append => {
                files
                    .entry(self.path.clone())
                    .or_default()
                    .extend_from_slice(&self.bytes);
            }
            WriteDisposition::CreateNew | WriteDisposition::CreateOrReplace => {
                files.insert(self.path.clone(), self.bytes.clone());
            }
        }
        let (atomicity, method) = if self.atomicity == AtomicityRequirement::NotRequired {
            (AchievedAtomicity::NonAtomic, PublicationMethod::Direct)
        } else {
            (AchievedAtomicity::Atomic, PublicationMethod::AtomicRename)
        };
        let mut outcome = WriteOutcome::new(atomicity, method);
        outcome.bytes_written = Some(self.bytes.len() as u64);
        Ok(outcome)
    }

    fn abort(&mut self) -> FsResult<()> {
        self.bytes.clear();
        Ok(())
    }
}
