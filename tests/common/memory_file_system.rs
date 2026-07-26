// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::{
    collections::{
        HashMap,
        VecDeque,
    },
    io::{
        Cursor,
        Write,
    },
    sync::{
        Arc,
        Mutex,
        atomic::{
            AtomicBool,
            Ordering,
        },
    },
};

use qubit_fs::{
    AchievedAtomicity,
    AtomicityRequirement,
    CopyConflictPolicy,
    CopyMethod,
    CopyOptions,
    CopyOutcome,
    CopyStats,
    CreateDirOptions,
    DeleteOptions,
    DirEntry,
    DirectoryStream,
    DirectoryStreamSession,
    FileKind,
    FileLocation,
    FileMetadata,
    FileReader,
    FileSystem,
    FileSystemCapabilities,
    FileSystemCapability,
    FileSystemId,
    FileSystemInfo,
    FileSystemLimits,
    FileSystemProperties,
    FileWriteSession,
    FileWriter,
    FsError,
    FsErrorKind,
    FsOperation,
    FsPath,
    FsResult,
    ListOptions,
    OpenedFileInfo,
    PathSemantics,
    PublicationMethod,
    ReadOptions,
    RenameOptions,
    RenameOutcome,
    WriteDisposition,
    WriteFailure,
    WriteOptions,
    WriteOutcome,
};
use qubit_fs_testkit::FileSystemFixture;

type Entries = Arc<Mutex<HashMap<String, MemoryEntry>>>;

#[derive(Clone)]
enum MemoryEntry {
    File(Vec<u8>),
    Directory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryFault {
    None,
    UnstableCapabilities,
    WrongStatKind,
    WrongReaderLocation,
    WrongWriterLocation,
    AppendReplaces,
    AtomicReplaceDowngrade,
    EmptyList,
    OmitListMetadata,
    SkipCreateDir,
    SkipDelete,
    CopyInsteadOfRename,
    MoveInsteadOfCopy,
    SkipReadPreflight,
}

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
                .with(FileSystemCapability::AtomicReplace)
                .with(FileSystemCapability::List)
                .with(FileSystemCapability::CreateDirectory)
                .with(FileSystemCapability::Delete)
                .with(FileSystemCapability::RecursiveDelete)
                .with(FileSystemCapability::Rename)
                .with(FileSystemCapability::AtomicRename)
                .with(FileSystemCapability::Copy),
        )
    }

    /// Creates an isolated filesystem with the specified capabilities.
    pub fn with_capabilities(capabilities: FileSystemCapabilities) -> Self {
        Self::with_capabilities_and_fault(capabilities, MemoryFault::None)
    }

    /// Creates an isolated filesystem with capabilities and one contract fault.
    pub fn with_capabilities_and_fault(
        capabilities: FileSystemCapabilities,
        fault: MemoryFault,
    ) -> Self {
        Self {
            file_system: MemoryFileSystem::new(capabilities, fault),
        }
    }

    /// Creates an isolated fully capable filesystem with one contract fault.
    #[allow(dead_code)]
    pub fn with_fault(fault: MemoryFault) -> Self {
        let capabilities = Self::new().file_system.capabilities;
        Self::with_capabilities_and_fault(capabilities, fault)
    }
}

impl FileSystemFixture for MemoryFixture {
    fn file_system(&self) -> &dyn FileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> FsPath {
        FsPath::parse(&format!("/{relative}"))
            .expect("contract fixture paths should parse")
    }
}

struct MemoryFileSystem {
    entries: Entries,
    info: FileSystemInfo,
    capabilities: FileSystemCapabilities,
    limits: FileSystemLimits,
    fault: MemoryFault,
    capability_flip: AtomicBool,
}

impl MemoryFileSystem {
    /// Creates an empty filesystem with the specified capabilities.
    fn new(capabilities: FileSystemCapabilities, fault: MemoryFault) -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            info: FileSystemInfo::new(
                FileSystemId::new("memory")
                    .expect("the memory filesystem ID should validate"),
                "memory-provider",
                PathSemantics::Hierarchical,
            ),
            capabilities,
            limits: FileSystemLimits::unknown(),
            fault,
            capability_flip: AtomicBool::new(false),
        }
    }

    /// Creates opened-file metadata for one path.
    fn opened_info(&self, path: &FsPath, writer: bool) -> OpenedFileInfo {
        let wrong_location = (writer
            && self.fault == MemoryFault::WrongWriterLocation)
            || (!writer && self.fault == MemoryFault::WrongReaderLocation);
        let location_path = if wrong_location {
            FsPath::parse("/wrong-contract-location")
                .expect("the deliberate wrong location should parse")
        } else {
            path.clone()
        };
        OpenedFileInfo::new(FileLocation::new(
            self.info.id().clone(),
            location_path,
        ))
    }

    /// Adds standard provider context to an option-validation error.
    fn with_context(&self, error: FsError, path: &FsPath) -> FsError {
        error
            .with_path(path.clone())
            .with_provider(self.info.provider_id())
    }

    /// Builds one contextual filesystem error.
    fn error(
        &self,
        kind: FsErrorKind,
        operation: FsOperation,
        path: &FsPath,
    ) -> FsError {
        FsError::new(kind, operation, "the memory operation failed")
            .with_path(path.clone())
            .with_provider(self.info.provider_id())
    }
}

impl FileSystemProperties for MemoryFileSystem {
    fn info(&self) -> &FileSystemInfo {
        &self.info
    }

    fn capabilities(&self) -> FileSystemCapabilities {
        if self.fault == MemoryFault::UnstableCapabilities
            && self.capability_flip.fetch_xor(true, Ordering::Relaxed)
        {
            self.capabilities.with(FileSystemCapability::TempFile)
        } else {
            self.capabilities
        }
    }

    fn limits(&self) -> &FileSystemLimits {
        &self.limits
    }
}

impl FileSystem for MemoryFileSystem {
    fn stat(&self, path: &FsPath) -> FsResult<FileMetadata> {
        let entries =
            self.entries.lock().expect("the memory store should lock");
        match entries.get(path.as_str()) {
            Some(MemoryEntry::File(bytes)) => {
                let kind = if self.fault == MemoryFault::WrongStatKind {
                    FileKind::Directory
                } else {
                    FileKind::File
                };
                let mut metadata = FileMetadata::new(kind);
                metadata.len = Some(bytes.len() as u64);
                Ok(metadata)
            }
            Some(MemoryEntry::Directory) => {
                Ok(FileMetadata::new(FileKind::Directory))
            }
            None => {
                Err(self.error(FsErrorKind::NotFound, FsOperation::Stat, path))
            }
        }
    }

    fn list(
        &self,
        path: &FsPath,
        options: ListOptions,
    ) -> FsResult<DirectoryStream> {
        let entries =
            self.entries.lock().expect("the memory store should lock");
        if !matches!(entries.get(path.as_str()), Some(MemoryEntry::Directory)) {
            return Err(self.error(
                FsErrorKind::NotFound,
                FsOperation::List,
                path,
            ));
        }
        let prefix = format!("{}/", path.as_str().trim_end_matches('/'));
        let mut listed = Vec::new();
        if self.fault == MemoryFault::EmptyList {
            return Ok(DirectoryStream::new(MemoryDirectoryStream {
                entries: VecDeque::new(),
            }));
        }
        for (entry_path, entry) in entries.iter() {
            let Some(relative) = entry_path.strip_prefix(&prefix) else {
                continue;
            };
            if relative.is_empty()
                || (!options.recursive && relative.contains('/'))
            {
                continue;
            }
            if let Some(filter) = &options.prefix
                && !relative
                    .rsplit('/')
                    .next()
                    .unwrap_or(relative)
                    .starts_with(filter)
            {
                continue;
            }
            let path = FsPath::parse(entry_path)
                .expect("stored memory paths should parse");
            let kind = match entry {
                MemoryEntry::File(_) => FileKind::File,
                MemoryEntry::Directory => FileKind::Directory,
            };
            let mut directory_entry = DirEntry::new(path, kind.clone());
            if options.include_metadata
                && self.fault != MemoryFault::OmitListMetadata
            {
                let mut metadata = FileMetadata::new(kind);
                if let MemoryEntry::File(bytes) = entry {
                    metadata.len = Some(bytes.len() as u64);
                }
                directory_entry.metadata = Some(metadata);
            }
            listed.push(directory_entry);
        }
        listed
            .sort_by(|left, right| left.path.as_str().cmp(right.path.as_str()));
        Ok(DirectoryStream::new(MemoryDirectoryStream {
            entries: VecDeque::from(listed),
        }))
    }

    fn open_reader(
        &self,
        path: &FsPath,
        options: ReadOptions,
    ) -> FsResult<FileReader> {
        if self.fault != MemoryFault::SkipReadPreflight {
            options
                .validate_against(self.capabilities)
                .map_err(|error| self.with_context(error, path))?;
        }
        let entries =
            self.entries.lock().expect("the memory store should lock");
        let bytes = match entries.get(path.as_str()) {
            Some(MemoryEntry::File(bytes)) => bytes.clone(),
            Some(MemoryEntry::Directory) => {
                return Err(self.error(
                    FsErrorKind::IsDirectory,
                    FsOperation::OpenReader,
                    path,
                ));
            }
            None => {
                return Err(self.error(
                    FsErrorKind::NotFound,
                    FsOperation::OpenReader,
                    path,
                ));
            }
        };
        Ok(FileReader::new(
            Cursor::new(bytes),
            self.opened_info(path, false),
        ))
    }

    fn open_writer(
        &self,
        path: &FsPath,
        options: WriteOptions,
    ) -> FsResult<FileWriter> {
        options
            .validate_against(self.capabilities)
            .map_err(|error| self.with_context(error, path))?;
        if options.disposition == WriteDisposition::CreateNew
            && self
                .entries
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
            entries: Arc::clone(&self.entries),
            path: path.as_str().to_owned(),
            create_parent: options.create_parent,
            disposition: options.disposition,
            atomicity: options.atomicity,
            fault: self.fault,
            bytes: Vec::new(),
        };
        Ok(FileWriter::new(session, self.opened_info(path, true)))
    }

    fn create_dir(
        &self,
        path: &FsPath,
        options: CreateDirOptions,
    ) -> FsResult<()> {
        if self.fault == MemoryFault::SkipCreateDir {
            return Ok(());
        }
        let mut entries =
            self.entries.lock().expect("the memory store should lock");
        if let Some(entry) = entries.get(path.as_str()) {
            return if options.exists_ok
                && matches!(entry, MemoryEntry::Directory)
            {
                Ok(())
            } else {
                Err(self.error(
                    FsErrorKind::AlreadyExists,
                    FsOperation::CreateDir,
                    path,
                ))
            };
        }
        if options.recursive {
            insert_parent_directories(&mut entries, path.as_str());
        }
        entries.insert(path.as_str().to_owned(), MemoryEntry::Directory);
        Ok(())
    }

    fn delete(&self, path: &FsPath, options: DeleteOptions) -> FsResult<()> {
        options
            .validate_against(self.capabilities)
            .map_err(|error| self.with_context(error, path))?;
        if self.fault == MemoryFault::SkipDelete {
            return Ok(());
        }
        let mut entries =
            self.entries.lock().expect("the memory store should lock");
        if !entries.contains_key(path.as_str()) {
            return if options.missing_ok {
                Ok(())
            } else {
                Err(self.error(
                    FsErrorKind::NotFound,
                    FsOperation::Delete,
                    path,
                ))
            };
        }
        let prefix = format!("{}/", path.as_str().trim_end_matches('/'));
        if !options.recursive
            && entries.keys().any(|entry| entry.starts_with(&prefix))
        {
            return Err(self.error(
                FsErrorKind::Conflict,
                FsOperation::Delete,
                path,
            ));
        }
        entries.retain(|entry, _| {
            entry != path.as_str() && !entry.starts_with(&prefix)
        });
        Ok(())
    }

    fn rename(
        &self,
        from: &FsPath,
        to: &FsPath,
        options: RenameOptions,
    ) -> FsResult<RenameOutcome> {
        options
            .validate_against(self.capabilities)
            .map_err(|error| self.with_context(error, from))?;
        let mut entries =
            self.entries.lock().expect("the memory store should lock");
        if entries.contains_key(to.as_str()) && !options.overwrite {
            return Err(self.error(
                FsErrorKind::AlreadyExists,
                FsOperation::Rename,
                from,
            ));
        }
        let entry = entries.get(from.as_str()).cloned().ok_or_else(|| {
            self.error(FsErrorKind::NotFound, FsOperation::Rename, from)
        })?;
        if self.fault != MemoryFault::CopyInsteadOfRename {
            entries.remove(from.as_str());
        }
        entries.insert(to.as_str().to_owned(), entry);
        Ok(RenameOutcome::new(
            AchievedAtomicity::Atomic,
            PublicationMethod::AtomicRename,
        ))
    }

    fn copy(
        &self,
        from: &FsPath,
        to: &FsPath,
        options: CopyOptions,
    ) -> FsResult<CopyOutcome> {
        options
            .validate_against(self.capabilities)
            .map_err(|error| self.with_context(error, from))?;
        let mut entries =
            self.entries.lock().expect("the memory store should lock");
        let bytes = match entries.get(from.as_str()) {
            Some(MemoryEntry::File(bytes)) => bytes.clone(),
            Some(MemoryEntry::Directory) => {
                return Err(self.error(
                    FsErrorKind::IsDirectory,
                    FsOperation::Copy,
                    from,
                ));
            }
            None => {
                return Err(self.error(
                    FsErrorKind::NotFound,
                    FsOperation::Copy,
                    from,
                ));
            }
        };
        if entries.contains_key(to.as_str())
            && options.conflict == CopyConflictPolicy::Fail
        {
            return Err(self.error(
                FsErrorKind::AlreadyExists,
                FsOperation::Copy,
                from,
            ));
        }
        entries
            .insert(to.as_str().to_owned(), MemoryEntry::File(bytes.clone()));
        if self.fault == MemoryFault::MoveInsteadOfCopy {
            entries.remove(from.as_str());
        }
        Ok(CopyOutcome::new(
            CopyStats {
                files: 1,
                bytes: bytes.len() as u64,
                ..CopyStats::default()
            },
            CopyMethod::Local,
            AchievedAtomicity::NonAtomic,
        ))
    }
}

struct MemoryWriteSession {
    entries: Entries,
    path: String,
    create_parent: bool,
    disposition: WriteDisposition,
    atomicity: AtomicityRequirement,
    fault: MemoryFault,
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
        let mut entries =
            self.entries.lock().expect("the memory store should lock");
        if self.create_parent {
            insert_parent_directories(&mut entries, &self.path);
        }
        match self.disposition {
            WriteDisposition::Append => {
                if self.fault == MemoryFault::AppendReplaces {
                    entries.insert(
                        self.path.clone(),
                        MemoryEntry::File(self.bytes.clone()),
                    );
                } else {
                    let entry = entries
                        .entry(self.path.clone())
                        .or_insert_with(|| MemoryEntry::File(Vec::new()));
                    if let MemoryEntry::File(bytes) = entry {
                        bytes.extend_from_slice(&self.bytes);
                    }
                }
            }
            WriteDisposition::CreateNew | WriteDisposition::CreateOrReplace => {
                entries.insert(
                    self.path.clone(),
                    MemoryEntry::File(self.bytes.clone()),
                );
            }
        }
        let (atomicity, method) = if self.fault
            == MemoryFault::AtomicReplaceDowngrade
            || self.atomicity == AtomicityRequirement::NotRequired
        {
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

struct MemoryDirectoryStream {
    entries: VecDeque<DirEntry>,
}

impl DirectoryStreamSession for MemoryDirectoryStream {
    fn next_entry(&mut self) -> FsResult<Option<DirEntry>> {
        Ok(self.entries.pop_front())
    }
}

/// Inserts every missing parent directory for one stored path.
fn insert_parent_directories(
    entries: &mut HashMap<String, MemoryEntry>,
    path: &str,
) {
    let components =
        path.trim_start_matches('/').split('/').collect::<Vec<_>>();
    let mut parent = String::new();
    for component in components.iter().take(components.len().saturating_sub(1))
    {
        parent.push('/');
        parent.push_str(component);
        entries
            .entry(parent.clone())
            .or_insert(MemoryEntry::Directory);
    }
}
