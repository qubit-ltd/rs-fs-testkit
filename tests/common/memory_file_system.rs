// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! A deliberately small SPI-backed provider used to self-test contract suites.

#![allow(dead_code)]

use std::collections::HashMap;
use std::future;
use std::io::{
    Cursor,
    Result as IoResult,
};
use std::pin::Pin;
use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
};
use std::task::{
    Context,
    Poll,
};

use qubit_fs::spi::{
    CopyAttempt,
    CopyDeclineReason,
    CreateDirectoryRequest,
    CreateTempDirectoryRequest,
    CreateTempFileRequest,
    DeleteDirectoryRequest,
    DeleteFileRequest,
    DirectoryStreamSpi,
    FileSystemSpi,
    FileWriterSpi,
    ListRequest,
    OpenReaderRequest,
    OpenWriterRequest,
    OpenedDirectoryStream,
    OpenedReader,
    OpenedTempDirectory,
    OpenedTempFile,
    OpenedWriter,
    PersistRequest,
    RenameRequest,
    SpiRenameFailure,
    SpiWriteFailure,
    StatRequest,
    StatResponse,
    TempResourceSpi,
};
use qubit_fs::{
    AchievedAtomicity,
    AtomicityRequirement,
    CopyMethod,
    CopyMode,
    CopyOutcome,
    CopyStats,
    CreateDirectoryOutcome,
    DeleteOutcome,
    FileKind,
    FileMetadata,
    FileSystem,
    FileSystemCapabilities,
    FileSystemCapability,
    FileSystemId,
    FileSystemInfo,
    FileSystemLimits,
    FileSystemProperties,
    FsError,
    FsErrorKind,
    FsOperation,
    FsResult,
    OpenedFileInfo,
    Path,
    PathConstraints,
    PathSemantics,
    PersistOutcome,
    PublicationMethod,
    RenameFailureState,
    RenameOutcome,
    ServerSidePreference,
    WriteDisposition,
    WriteFailureState,
    WriteOutcome,
    WritePrecondition,
};
use qubit_fs_testkit::{
    AsyncCopyCancellationStage,
    AsyncCopyFixtureCase,
    AsyncFileSystemFixture,
    CopyFixtureCase,
    FixtureFuture,
};
use qubit_fs_testkit::{
    FileSystemFixture,
    FixtureResult,
    FixtureSupport,
};
use qubit_io::{
    AsyncInput,
    AsyncOutput,
    Output,
};

/// One isolated contract fixture backed by the public synchronous facade.
pub struct MemoryFixture {
    file_system: FileSystem,
    state: Arc<Mutex<State>>,
    path_calls: Arc<AtomicUsize>,
}

/// A single injected provider defect used by the self-test matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryFault {
    /// Behaves conformingly.
    None,
    /// Returns directory metadata for an existing file.
    WrongStatKind,
    /// Leaves a cleaned temporary resource in the namespace.
    KeepTempOnCleanup,
    /// Reports a persisted target different from the requested target.
    WrongPersistTarget,
    /// Returns no entries for a non-empty requested directory.
    EmptyList,
    /// Returns bytes different from the provider's stored content.
    ReadWrongBytes,
    /// Accepts writes without publishing their content.
    WriteDropsBytes,
    /// Reports deletion success without removing the resource.
    DeleteNoOp,
    /// Reports rename success without moving the resource.
    RenameNoOp,
    /// Omits eagerly requested listing metadata.
    ListDropsMetadata,
    /// Copies a directory root without its descendants.
    DirectoryCopyDropsChildren,
    /// Ignores temporary-resource parent and affix options.
    TempIgnoresOptions,
    /// Uses object and prefix metadata kinds for stored resources.
    ObjectKinds,
    /// Overwrites instead of appending to an existing file.
    AppendOverwrites,
    /// Reports non-atomic completion for an atomic-required rename.
    AtomicRenameNonAtomic,
    /// Reports non-atomic completion for an atomic-required write.
    AtomicReplaceNonAtomic,
    /// Reports a non-durable completion for a durability-required copy.
    DurableCopyNonDurable,
    /// Reports non-atomic completion for an atomic-required temp persist.
    AtomicTempPersistNonAtomic,
    /// Reports a non-server-side completion for a server-side-required copy.
    ServerSideCopyFallsBack,
    /// Removes the requested directory but leaves recursive descendants.
    RecursiveDeleteLeavesChildren,
}

#[derive(Clone)]
enum Entry {
    File(Vec<u8>),
    Directory,
    Symlink,
}

struct State {
    entries: HashMap<String, Entry>,
    next_temp: u64,
    fault: MemoryFault,
    delete_capability: bool,
    core_capabilities: bool,
    optional_capabilities: bool,
    create_directory_capability: bool,
    extended_capabilities: bool,
    native_copy: bool,
}

impl MemoryFixture {
    /// Creates a fresh conforming fixture.
    pub fn new() -> Self {
        Self::with_fault(MemoryFault::None)
    }

    /// Creates a conforming fixture whose copy primitive completes natively.
    pub fn with_native_copy() -> Self {
        let fixture = Self::new();
        fixture
            .state
            .lock()
            .expect("memory state lock must succeed")
            .native_copy = true;
        fixture
    }

    /// Creates a fresh fixture with exactly one provider fault.
    pub fn with_fault(fault: MemoryFault) -> Self {
        Self::with_capabilities(fault, true, true)
    }

    /// Creates a fixture whose facade does not advertise deletion.
    pub fn without_delete() -> Self {
        Self::with_capabilities(MemoryFault::None, false, true)
    }

    /// Creates a fixture without the core read, write, list, and copy
    /// capabilities.
    pub fn without_core_capabilities() -> Self {
        Self::with_capabilities(MemoryFault::None, true, false)
    }

    /// Creates a fixture that advertises none of the suite operation
    /// capabilities.
    pub fn without_operation_capabilities() -> Self {
        Self::with_configuration(
            MemoryFault::None,
            false,
            false,
            false,
            false,
            false,
            "memory-contract-provider",
        )
    }

    /// Creates a fixture that exposes only the core capabilities.
    pub fn without_optional_capabilities() -> Self {
        Self::with_configuration(
            MemoryFault::None,
            true,
            true,
            false,
            false,
            false,
            "memory-contract-provider",
        )
    }

    /// Creates a conforming fixture whose filesystem and provider identifiers
    /// are identical.
    pub fn with_matching_ids() -> Self {
        Self::with_configuration(
            MemoryFault::None,
            true,
            true,
            true,
            true,
            false,
            "memory-contract",
        )
    }

    /// Creates a fixture using object and prefix metadata kinds.
    pub fn with_object_kinds() -> Self {
        Self::with_configuration(
            MemoryFault::ObjectKinds,
            true,
            true,
            true,
            true,
            false,
            "memory-object-provider",
        )
    }

    /// Creates a fixture supporting recursive prefix deletion without directory
    /// creation.
    pub fn recursive_delete_without_create_directory() -> Self {
        Self::with_configuration(
            MemoryFault::None,
            true,
            true,
            true,
            false,
            false,
            "memory-prefix-provider",
        )
    }

    /// Creates a fixture advertising every capability contract.
    pub fn with_all_capabilities() -> Self {
        Self::with_configuration(
            MemoryFault::None,
            true,
            true,
            true,
            true,
            true,
            "memory-all-capabilities-provider",
        )
    }

    /// Creates a fixture with one injected fault and deletion-capability value.
    ///
    /// `fault` selects the provider defect; `delete_capability` controls the
    /// facade property snapshot returned to the suite.
    fn with_capabilities(
        fault: MemoryFault,
        delete_capability: bool,
        core_capabilities: bool,
    ) -> Self {
        Self::with_configuration(
            fault,
            delete_capability,
            core_capabilities,
            true,
            true,
            false,
            "memory-contract-provider",
        )
    }

    /// Creates a fixture with selected capabilities and provider identity.
    fn with_configuration(
        fault: MemoryFault,
        delete_capability: bool,
        core_capabilities: bool,
        optional_capabilities: bool,
        create_directory_capability: bool,
        extended_capabilities: bool,
        provider_id: &'static str,
    ) -> Self {
        let state = Arc::new(Mutex::new(State {
            entries: HashMap::new(),
            next_temp: 0,
            fault,
            delete_capability,
            core_capabilities,
            optional_capabilities,
            create_directory_capability,
            extended_capabilities,
            native_copy: false,
        }));
        let path_calls = Arc::new(AtomicUsize::new(0));
        let file_system = FileSystem::from_spi(MemorySpi {
            state: Arc::clone(&state),
            provider_id,
        })
        .expect("memory SPI properties must be valid");
        Self {
            file_system,
            state,
            path_calls,
        }
    }

    /// Builds one absolute logical path for the fixture namespace.
    fn path_for(relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/contract/{relative}")).map_err(|error| {
            qubit_fs_testkit::FixtureError::new(error.to_string())
        })
    }

    /// Returns whether the fixture namespace contains no resources.
    pub fn is_empty(&self) -> bool {
        self.entry_count() == 0
    }

    /// Returns the number of resources retained by the fixture namespace.
    pub fn entry_count(&self) -> usize {
        self.state
            .lock()
            .expect("memory state lock must succeed")
            .entries
            .len()
    }

    /// Returns how many contract paths the suite requested from this fixture.
    pub fn path_call_count(&self) -> usize {
        self.path_calls.load(Ordering::Relaxed)
    }
}

impl FileSystemFixture for MemoryFixture {
    fn file_system(&self) -> &FileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.path_calls.fetch_add(1, Ordering::Relaxed);
        Self::path_for(relative)
    }

    fn seed_file(
        &self,
        relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<FixtureSupport<Path>> {
        let path = Self::path_for(relative)?;
        self.state
            .lock()
            .expect("memory state lock must succeed")
            .entries
            .insert(path.as_str().to_owned(), Entry::File(bytes.to_vec()));
        Ok(FixtureSupport::Supported(path))
    }

    fn read_file(&self, path: &Path) -> FixtureResult<FixtureSupport<Vec<u8>>> {
        let entry = self
            .state
            .lock()
            .expect("memory state lock must succeed")
            .entries
            .get(path.as_str())
            .cloned();
        Ok(match entry {
            Some(Entry::File(bytes)) => FixtureSupport::Supported(bytes),
            Some(Entry::Directory | Entry::Symlink) | None => {
                FixtureSupport::Unsupported
            }
        })
    }

    fn resource_version(
        &self,
        path: &Path,
    ) -> FixtureResult<FixtureSupport<qubit_fs::ResourceVersion>> {
        let exists = self
            .state
            .lock()
            .expect("memory state lock must succeed")
            .entries
            .contains_key(path.as_str());
        Ok(if exists {
            FixtureSupport::Supported(qubit_fs::ResourceVersion::new("v1"))
        } else {
            FixtureSupport::Unsupported
        })
    }

    fn seed_empty_directory(
        &self,
        relative: &str,
    ) -> FixtureResult<FixtureSupport<Path>> {
        let path = Self::path_for(relative)?;
        self.state
            .lock()
            .expect("memory state lock must succeed")
            .entries
            .insert(path.as_str().to_owned(), Entry::Directory);
        Ok(FixtureSupport::Supported(path))
    }

    fn seed_symlink(
        &self,
        relative: &str,
    ) -> FixtureResult<FixtureSupport<Path>> {
        let path = Self::path_for(relative)?;
        self.state
            .lock()
            .expect("memory state lock must succeed")
            .entries
            .insert(path.as_str().to_owned(), Entry::Symlink);
        Ok(FixtureSupport::Supported(path))
    }

    fn copy_fast_path_case(
        &self,
        method: CopyMethod,
    ) -> FixtureResult<FixtureSupport<qubit_fs_testkit::CopyFixtureCase>> {
        if method != CopyMethod::ServerSide {
            return Ok(FixtureSupport::Unsupported);
        }
        let source = Self::path_for("server-side-copy-source")?;
        let target = Self::path_for("server-side-copy-target")?;
        self.state
            .lock()
            .expect("memory state lock must succeed")
            .entries
            .insert(
                source.as_str().to_owned(),
                Entry::File(b"server-side".to_vec()),
            );
        Ok(FixtureSupport::Supported(
            qubit_fs_testkit::CopyFixtureCase::new(
                source,
                target,
                qubit_fs::CopyOptions::default()
                    .with_server_side(ServerSidePreference::Require),
            ),
        ))
    }
}

struct MemorySpi {
    state: Arc<Mutex<State>>,
    provider_id: &'static str,
}

impl MemorySpi {
    /// Returns a safe provider error for an unsupported primitive.
    fn unsupported(operation: FsOperation) -> FsError {
        FsError::new(
            FsErrorKind::UnsupportedOperation,
            operation,
            "unused memory SPI operation",
        )
    }

    /// Returns the provider identity for an opened temporary resource.
    fn info(path: Path) -> OpenedFileInfo {
        OpenedFileInfo::new(
            FileSystemId::new("memory-contract")
                .expect("memory provider id must be valid"),
            path,
        )
    }

    /// Allocates one temporary resource path and inserts its entry.
    fn create_temp(
        &self,
        directory: bool,
        parent: Option<&Path>,
        prefix: &str,
        suffix: &str,
    ) -> Path {
        let mut state =
            self.state.lock().expect("memory state lock must succeed");
        let parent = if state.fault == MemoryFault::TempIgnoresOptions {
            "/contract"
        } else {
            parent.map_or("/contract", Path::as_str)
        };
        let prefix = if state.fault == MemoryFault::TempIgnoresOptions {
            if directory { ".tmp-dir-" } else { ".tmp-" }
        } else {
            prefix
        };
        let suffix = if state.fault == MemoryFault::TempIgnoresOptions {
            ""
        } else {
            suffix
        };
        let path = Path::parse(&format!(
            "{parent}/{prefix}{}{suffix}",
            state.next_temp
        ))
        .expect("generated temporary path must be valid");
        state.next_temp += 1;
        state.entries.insert(
            path.as_str().to_owned(),
            if directory {
                Entry::Directory
            } else {
                Entry::File(Vec::new())
            },
        );
        path
    }
}

impl FileSystemSpi for MemorySpi {
    fn properties(&self) -> FileSystemProperties {
        let state = self.state.lock().expect("memory state lock must succeed");
        let mut capabilities = FileSystemCapabilities::new();
        if state.optional_capabilities {
            capabilities = capabilities
                .with(FileSystemCapability::Rename)
                .with(FileSystemCapability::TempFile)
                .with(FileSystemCapability::TempDirectory)
                .with(FileSystemCapability::Append)
                .with(FileSystemCapability::AtomicRename)
                .with(FileSystemCapability::AtomicReplace)
                .with(FileSystemCapability::DurableCopy)
                .with(FileSystemCapability::AtomicTempPersist)
                .with(FileSystemCapability::ServerSideCopy);
        }
        if state.create_directory_capability {
            capabilities =
                capabilities.with(FileSystemCapability::CreateDirectory);
        }
        if state.extended_capabilities {
            for capability in [
                FileSystemCapability::RangeRead,
                FileSystemCapability::ConditionalRead,
                FileSystemCapability::ChecksumValidation,
                FileSystemCapability::ConditionalWrite,
                FileSystemCapability::EmptyDirectory,
                FileSystemCapability::ConditionalDelete,
                FileSystemCapability::Symlink,
            ] {
                capabilities.insert(capability);
            }
        }
        if state.core_capabilities {
            capabilities = capabilities
                .with(FileSystemCapability::Read)
                .with(FileSystemCapability::Write)
                .with(FileSystemCapability::List)
                .with(FileSystemCapability::Copy);
        }
        if state.delete_capability {
            capabilities = capabilities.with(FileSystemCapability::Delete);
            if state.optional_capabilities {
                capabilities =
                    capabilities.with(FileSystemCapability::RecursiveDelete);
            }
        }
        drop(state);
        FileSystemProperties::new(
            FileSystemInfo::new(
                FileSystemId::new("memory-contract")
                    .expect("memory provider id must be valid"),
                self.provider_id,
                PathSemantics::Hierarchical,
            ),
            capabilities,
            FileSystemLimits::unknown(),
            PathConstraints::absolute(),
        )
        .expect("memory properties must be valid")
    }

    fn stat(&self, request: StatRequest<'_>) -> FsResult<StatResponse> {
        let state = self.state.lock().expect("memory state lock must succeed");
        let Some(entry) = state.entries.get(request.path().as_str()) else {
            return Err(FsError::new(
                FsErrorKind::NotFound,
                FsOperation::Stat,
                "memory entry absent",
            ));
        };
        let kind = match (state.fault, entry) {
            (MemoryFault::WrongStatKind, Entry::File(_)) => FileKind::Directory,
            (MemoryFault::ObjectKinds, Entry::File(_)) => FileKind::Object,
            (MemoryFault::ObjectKinds, Entry::Directory) => FileKind::Prefix,
            (_, Entry::File(_)) => FileKind::File,
            (_, Entry::Directory) => FileKind::Directory,
            (_, Entry::Symlink) => FileKind::Symlink,
        };
        let mut metadata = FileMetadata::new(kind);
        if let Entry::File(bytes) = entry {
            metadata = metadata.with_len(Some(bytes.len() as u64));
        }
        Ok(StatResponse::new(request.path().clone(), metadata))
    }

    fn list(
        &self,
        request: ListRequest<'_>,
    ) -> FsResult<OpenedDirectoryStream> {
        let state = self.state.lock().expect("memory state lock must succeed");
        let entries = if state.fault == MemoryFault::EmptyList {
            Vec::new()
        } else {
            listed_entries(
                &state.entries,
                request.path(),
                request.options().options(),
                state.fault != MemoryFault::ListDropsMetadata,
            )
        };
        Ok(OpenedDirectoryStream::new(Box::new(
            MemoryDirectoryStream {
                entries: entries.into_iter(),
            },
        )))
    }

    fn open_reader(
        &self,
        request: OpenReaderRequest<'_>,
    ) -> FsResult<OpenedReader> {
        let state = self.state.lock().expect("memory state lock must succeed");
        let Some(Entry::File(bytes)) =
            state.entries.get(request.path().as_str())
        else {
            return Err(FsError::new(
                FsErrorKind::NotFound,
                FsOperation::OpenReader,
                "memory entry absent",
            ));
        };
        if request
            .options()
            .options()
            .if_match()
            .as_ref()
            .is_some_and(|version| version.as_str() != "v1")
            || request
                .options()
                .options()
                .if_none_match()
                .as_ref()
                .is_some_and(|version| version.as_str() == "v1")
        {
            return Err(FsError::new(
                FsErrorKind::PreconditionFailed,
                FsOperation::OpenReader,
                "memory read condition failed",
            ));
        }
        let mut bytes = if state.fault == MemoryFault::ReadWrongBytes {
            b"wrong bytes".to_vec()
        } else {
            bytes.clone()
        };
        let options = request.options().options();
        let start =
            options.offset().unwrap_or(0).min(bytes.len() as u64) as usize;
        let end = options.length().map_or(bytes.len(), |length| {
            start.saturating_add(length as usize).min(bytes.len())
        });
        bytes = bytes[start..end].to_vec();
        Ok(OpenedReader::new(
            Self::info(request.path().clone()),
            Box::new(Cursor::new(bytes)),
        ))
    }

    fn open_writer(
        &self,
        request: OpenWriterRequest<'_>,
    ) -> FsResult<OpenedWriter> {
        Ok(OpenedWriter::new(
            Self::info(request.path().clone()),
            Box::new(MemoryWriter {
                state: Arc::clone(&self.state),
                path: request.path().clone(),
                bytes: Vec::new(),
                disposition: request.options().options().disposition(),
                atomicity: request.options().options().atomicity(),
                precondition: request.options().options().precondition().clone(),
            }),
        ))
    }

    fn create_directory(
        &self,
        request: CreateDirectoryRequest<'_>,
    ) -> FsResult<CreateDirectoryOutcome> {
        let mut state =
            self.state.lock().expect("memory state lock must succeed");
        let already_existed =
            state.entries.contains_key(request.path().as_str());
        state
            .entries
            .entry(request.path().as_str().to_owned())
            .or_insert(Entry::Directory);
        Ok(CreateDirectoryOutcome::new(already_existed))
    }

    fn delete_file(
        &self,
        request: DeleteFileRequest<'_>,
    ) -> FsResult<DeleteOutcome> {
        let mut state =
            self.state.lock().expect("memory state lock must succeed");
        let removed = if state.fault == MemoryFault::DeleteNoOp {
            None
        } else {
            state.entries.remove(request.path().as_str())
        };
        Ok(DeleteOutcome::new(removed.is_none()))
    }

    fn delete_directory(
        &self,
        request: DeleteDirectoryRequest<'_>,
    ) -> FsResult<DeleteOutcome> {
        let mut state =
            self.state.lock().expect("memory state lock must succeed");
        let already_missing = if state.fault == MemoryFault::DeleteNoOp {
            true
        } else {
            let removed = state.entries.remove(request.path().as_str());
            let mut removed_descendant = false;
            if request.options().options().recursive()
                && state.fault != MemoryFault::RecursiveDeleteLeavesChildren
            {
                let prefix = format!(
                    "{}/",
                    request.path().as_str().trim_end_matches('/')
                );
                let before = state.entries.len();
                state.entries.retain(|path, _| !path.starts_with(&prefix));
                removed_descendant = state.entries.len() != before;
            }
            removed.is_none() && !removed_descendant
        };
        Ok(DeleteOutcome::new(already_missing))
    }

    fn try_copy(
        &self,
        request: qubit_fs::spi::CopyRequest<'_>,
    ) -> Result<CopyAttempt, qubit_fs::spi::SpiCopyFailure> {
        let mut state =
            self.state.lock().expect("memory state lock must succeed");
        let options = request.options().options();
        if state.native_copy
            || options.mode() == qubit_fs::CopyMode::Tree
            || options.server_side() == ServerSidePreference::Require
            || options.durability() == qubit_fs::DurabilityRequirement::Required
            || options.conflict() == qubit_fs::CopyConflictPolicy::Overwrite
        {
            let Some(entry) =
                state.entries.get(request.source().as_str()).cloned()
            else {
                return Err(qubit_fs::spi::SpiCopyFailure::new(
                    FsError::new(
                        FsErrorKind::NotFound,
                        FsOperation::Copy,
                        "memory copy source absent",
                    ),
                    qubit_fs::CopyFailureState::Unchanged,
                    CopyStats::default(),
                ));
            };
            if state.entries.contains_key(request.target().as_str()) {
                match options.conflict() {
                    qubit_fs::CopyConflictPolicy::Fail => {
                        return Err(qubit_fs::spi::SpiCopyFailure::new(
                            FsError::new(
                                FsErrorKind::AlreadyExists,
                                FsOperation::Copy,
                                "memory copy target already exists",
                            ),
                            qubit_fs::CopyFailureState::Unchanged,
                            CopyStats::default(),
                        ));
                    }
                    qubit_fs::CopyConflictPolicy::Skip => {
                        return Ok(CopyAttempt::Completed(CopyOutcome::new(
                            CopyStats {
                                skipped: 1,
                                ..CopyStats::default()
                            },
                            CopyMethod::Native,
                            AchievedAtomicity::NonAtomic,
                        )));
                    }
                    qubit_fs::CopyConflictPolicy::Overwrite => {}
                }
            }
            let bytes = match &entry {
                Entry::File(bytes) => bytes.len() as u64,
                Entry::Directory | Entry::Symlink => 0,
            };
            let overwritten = state
                .entries
                .contains_key(request.target().as_str())
                && options.conflict() == qubit_fs::CopyConflictPolicy::Overwrite;
            state
                .entries
                .insert(request.target().as_str().to_owned(), entry);
            if matches!(
                state.entries.get(request.source().as_str()),
                Some(Entry::Directory)
            ) && state.fault != MemoryFault::DirectoryCopyDropsChildren
            {
                let source_prefix = format!(
                    "{}/",
                    request.source().as_str().trim_end_matches('/')
                );
                let target_prefix = format!(
                    "{}/",
                    request.target().as_str().trim_end_matches('/')
                );
                let descendants = state
                    .entries
                    .iter()
                    .filter_map(|(path, entry)| {
                        path.strip_prefix(&source_prefix).map(|relative| {
                            (
                                format!("{target_prefix}{relative}"),
                                entry.clone(),
                            )
                        })
                    })
                    .collect::<Vec<_>>();
                for (path, entry) in descendants {
                    state.entries.insert(path, entry);
                }
            }
            let method = if options.server_side() == ServerSidePreference::Require
                && state.fault != MemoryFault::ServerSideCopyFallsBack
            {
                CopyMethod::ServerSide
            } else {
                CopyMethod::Native
            };
            return Ok(CopyAttempt::Completed(
                CopyOutcome::new(
                    CopyStats {
                        files: 1,
                        bytes,
                        overwritten: u64::from(overwritten),
                        ..CopyStats::default()
                    },
                    method,
                    if options.atomicity() == AtomicityRequirement::Required {
                        AchievedAtomicity::Atomic
                    } else {
                        AchievedAtomicity::NonAtomic
                    },
                )
                .with_durable(
                    options.durability()
                        == qubit_fs::DurabilityRequirement::Required
                        && state.fault != MemoryFault::DurableCopyNonDurable,
                ),
            ));
        }
        Ok(CopyAttempt::Declined(CopyDeclineReason::NotImplemented))
    }

    fn rename(
        &self,
        request: RenameRequest<'_>,
    ) -> Result<RenameOutcome, SpiRenameFailure> {
        let mut state =
            self.state.lock().expect("memory state lock must succeed");
        if !request.options().options().overwrite()
            && state.entries.contains_key(request.target().as_str())
        {
            return Err(SpiRenameFailure::new(
                FsError::new(
                    FsErrorKind::AlreadyExists,
                    FsOperation::Rename,
                    "memory rename target already exists",
                ),
                RenameFailureState::Unchanged,
            ));
        }
        let Some(entry) = state.entries.remove(request.source().as_str())
        else {
            return Err(SpiRenameFailure::new(
                FsError::new(
                    FsErrorKind::NotFound,
                    FsOperation::Rename,
                    "memory entry absent",
                ),
                RenameFailureState::Unchanged,
            ));
        };
        if state.fault != MemoryFault::RenameNoOp {
            state
                .entries
                .insert(request.target().as_str().to_owned(), entry);
        } else {
            state
                .entries
                .insert(request.source().as_str().to_owned(), entry);
        }
        Ok(RenameOutcome::new(
            request.source().clone(),
            request.target().clone(),
            if request.options().options().atomicity()
                == AtomicityRequirement::Required
                && state.fault != MemoryFault::AtomicRenameNonAtomic
            {
                AchievedAtomicity::Atomic
            } else {
                AchievedAtomicity::NonAtomic
            },
            PublicationMethod::Direct,
        ))
    }

    fn create_temp_file(
        &self,
        request: CreateTempFileRequest,
    ) -> FsResult<OpenedTempFile> {
        let path = self.create_temp(
            false,
            request.options().parent(),
            request.options().prefix(),
            request.options().suffix(),
        );
        Ok(OpenedTempFile::new(
            Self::info(path.clone())
                .with_metadata(FileMetadata::new(FileKind::File)),
            Box::new(TempSession {
                state: Arc::clone(&self.state),
                path,
            }),
        ))
    }

    fn create_temp_directory(
        &self,
        request: CreateTempDirectoryRequest,
    ) -> FsResult<OpenedTempDirectory> {
        let path = self.create_temp(
            true,
            request.options().parent(),
            request.options().prefix(),
            request.options().suffix(),
        );
        Ok(OpenedTempDirectory::new(
            Self::info(path.clone())
                .with_metadata(FileMetadata::new(FileKind::Directory)),
            Box::new(TempSession {
                state: Arc::clone(&self.state),
                path,
            }),
        ))
    }
}

struct MemoryDirectoryStream {
    entries: std::vec::IntoIter<qubit_fs::DirEntry>,
}

impl DirectoryStreamSpi for MemoryDirectoryStream {
    fn next_entry(&mut self) -> FsResult<Option<qubit_fs::DirEntry>> {
        Ok(self.entries.next())
    }
}

fn listed_entries(
    entries: &HashMap<String, Entry>,
    root: &Path,
    options: &qubit_fs::ListOptions,
    include_metadata: bool,
) -> Vec<qubit_fs::DirEntry> {
    let prefix = format!("{}/", root.as_str().trim_end_matches('/'));
    entries
        .iter()
        .filter_map(|(text, entry)| {
            let relative = text.strip_prefix(&prefix)?;
            if relative.is_empty()
                || (!options.recursive()
                    && options.prefix().is_none()
                    && relative.contains('/'))
            {
                return None;
            }
            if !options.prefix().is_none_or(|prefix| {
                relative == prefix
                    || relative
                        .strip_prefix(prefix)
                        .is_some_and(|remaining| remaining.starts_with('/'))
            }) {
                return None;
            }
            let kind = match entry {
                Entry::File(_) => FileKind::File,
                Entry::Directory => FileKind::Directory,
                Entry::Symlink => FileKind::Symlink,
            };
            let mut result = qubit_fs::DirEntry::new(
                Path::parse(text)
                    .expect("stored memory path must remain valid"),
                kind.clone(),
            );
            if options.include_metadata() && include_metadata {
                let mut metadata = FileMetadata::new(kind);
                if let Entry::File(bytes) = entry {
                    metadata = metadata.with_len(Some(bytes.len() as u64));
                }
                result.metadata = Some(metadata);
            }
            Some(result)
        })
        .collect()
}

struct MemoryWriter {
    state: Arc<Mutex<State>>,
    path: Path,
    bytes: Vec<u8>,
    disposition: WriteDisposition,
    atomicity: AtomicityRequirement,
    precondition: WritePrecondition,
}

impl Output for MemoryWriter {
    type Item = u8;

    unsafe fn write_unchecked(
        &mut self,
        input: &[u8],
        index: usize,
        count: usize,
    ) -> IoResult<usize> {
        self.bytes.extend_from_slice(&input[index..index + count]);
        Ok(count)
    }

    fn flush(&mut self) -> IoResult<()> {
        Ok(())
    }
}

impl FileWriterSpi for MemoryWriter {
    fn commit(&mut self) -> Result<WriteOutcome, SpiWriteFailure> {
        let mut state =
            self.state.lock().expect("memory state lock must succeed");
        if self.disposition == WriteDisposition::CreateNew
            && state.entries.contains_key(self.path.as_str())
        {
            return Err(SpiWriteFailure::new(
                FsError::new(
                    FsErrorKind::AlreadyExists,
                    FsOperation::CommitWriter,
                    "memory destination already exists",
                ),
                WriteFailureState::NotPublished,
            ));
        }
        if self.precondition == WritePrecondition::IfAbsent
            && state.entries.contains_key(self.path.as_str())
        {
            return Err(SpiWriteFailure::new(
                FsError::new(
                    FsErrorKind::PreconditionFailed,
                    FsOperation::CommitWriter,
                    "memory destination violates if-absent",
                ),
                WriteFailureState::NotPublished,
            ));
        }
        if state.fault != MemoryFault::WriteDropsBytes {
            let bytes = if self.disposition == WriteDisposition::Append
                && state.fault != MemoryFault::AppendOverwrites
            {
                match state.entries.get(self.path.as_str()) {
                    Some(Entry::File(existing)) => {
                        [existing.as_slice(), self.bytes.as_slice()].concat()
                    }
                    Some(Entry::Directory | Entry::Symlink) | None => {
                        self.bytes.clone()
                    }
                }
            } else {
                self.bytes.clone()
            };
            state
                .entries
                .insert(self.path.as_str().to_owned(), Entry::File(bytes));
        }
        Ok(WriteOutcome::new(
            if self.atomicity == AtomicityRequirement::Required
                && state.fault != MemoryFault::AtomicReplaceNonAtomic
            {
                AchievedAtomicity::Atomic
            } else {
                AchievedAtomicity::NonAtomic
            },
            PublicationMethod::Direct,
        ))
    }

    fn abort(&mut self) -> FsResult<()> {
        Ok(())
    }
}

struct TempSession {
    state: Arc<Mutex<State>>,
    path: Path,
}

impl TempResourceSpi for TempSession {
    fn persist(
        &mut self,
        request: PersistRequest<'_>,
    ) -> Result<PersistOutcome, qubit_fs::spi::SpiPersistFailure> {
        let mut state =
            self.state.lock().expect("memory state lock must succeed");
        let entry = state
            .entries
            .remove(self.path.as_str())
            .expect("temporary entry must exist");
        state
            .entries
            .insert(request.target().as_str().to_owned(), entry);
        let target = if state.fault == MemoryFault::WrongPersistTarget {
            Path::parse("/contract/wrong-persist-target")
                .expect("generated path must be valid")
        } else {
            request.target().clone()
        };
        Ok(PersistOutcome::new(
            target,
            if request.options().atomicity() == AtomicityRequirement::Required
                && state.fault != MemoryFault::AtomicTempPersistNonAtomic
            {
                AchievedAtomicity::Atomic
            } else {
                AchievedAtomicity::NonAtomic
            },
            PublicationMethod::Direct,
        ))
    }

    fn keep(&mut self) -> FsResult<()> {
        Ok(())
    }

    fn cleanup(&mut self) -> FsResult<()> {
        let mut state =
            self.state.lock().expect("memory state lock must succeed");
        if state.fault != MemoryFault::KeepTempOnCleanup {
            state.entries.remove(self.path.as_str());
        }
        Ok(())
    }
}

/// Async fixture whose copy pipeline exposes one real pending point per stage.
pub struct AsyncMemoryFixture {
    file_system: qubit_fs::AsyncFileSystem,
    stage: Arc<Mutex<AsyncCopyCancellationStage>>,
    entries: Arc<Mutex<HashMap<String, Entry>>>,
    supports_cancellation_cases: bool,
    path_calls: Arc<AtomicUsize>,
}

/// A single injected asynchronous provider defect used by the self-test matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AsyncMemoryFault {
    /// Behaves conformingly.
    None,
    /// Reports a missing path as an existing file.
    MissingPathExists,
    /// Returns directory metadata for an existing file.
    WrongStatMetadata,
    /// Returns bytes different from the provider's seeded content.
    ReadWrongBytes,
    /// Accepts writes but does not publish their bytes.
    WriteDropsBytes,
    /// Produces a listing entry outside the requested namespace.
    ListEscapesNamespace,
    /// Returns no entries for a non-empty requested directory.
    EmptyList,
    /// Omits metadata explicitly requested by the caller.
    ListDropsMetadata,
    /// Reports deletion success without removing the resource.
    DeleteNoOp,
    /// Reports copy success without publishing the target bytes.
    CopyDropsTarget,
    /// Reports rename success without moving the resource.
    RenameNoOp,
    /// Reports a rename outcome with identities different from the request.
    RenameWrongOutcome,
    /// Copies a directory root without its descendants.
    DirectoryCopyDropsChildren,
    /// Reports temporary cleanup success without removing the resource.
    TempCleanupNoOp,
    /// Appends by replacing existing bytes.
    AppendOverwrites,
    /// Removes only the requested directory during recursive deletion.
    RecursiveDeleteLeavesChildren,
    /// Reports non-atomic completion for a required atomic rename.
    AtomicRenameNonAtomic,
    /// Reports non-atomic completion for a required atomic replacement.
    AtomicReplaceNonAtomic,
    /// Reports non-durable completion for a required durable copy.
    DurableCopyNonDurable,
    /// Publishes the requested destination but reports a different target.
    TempPersistWrongTarget,
    /// Reports non-atomic completion for required temporary persistence.
    AtomicTempPersistNonAtomic,
    /// Ignores temporary-resource parent and affix options.
    TempIgnoresOptions,
    /// Uses object and prefix metadata kinds for stored resources.
    ObjectKinds,
}

/// Capability switches used by asynchronous memory fixture profiles.
#[derive(Clone, Copy)]
struct AsyncCapabilityProfile {
    core: bool,
    optional: bool,
    create_directory: bool,
    extended: bool,
}

impl AsyncCapabilityProfile {
    const NONE: Self = Self {
        core: false,
        optional: false,
        create_directory: false,
        extended: false,
    };
    const CORE: Self = Self {
        core: true,
        optional: false,
        create_directory: false,
        extended: false,
    };
    const STANDARD: Self = Self {
        core: true,
        optional: true,
        create_directory: true,
        extended: false,
    };
    const PREFIX_DELETE: Self = Self {
        core: true,
        optional: true,
        create_directory: false,
        extended: false,
    };
    const ALL: Self = Self {
        core: true,
        optional: true,
        create_directory: true,
        extended: true,
    };
}

impl AsyncMemoryFixture {
    /// Creates an isolated asynchronous copy fixture.
    pub fn new() -> Self {
        Self::with_fault(AsyncMemoryFault::None)
    }

    /// Creates an asynchronous fixture with exactly one provider fault.
    pub fn with_fault(fault: AsyncMemoryFault) -> Self {
        Self::with_copy_behavior(fault, true, false)
    }

    /// Creates a conforming fixture without optional cancellation probes.
    pub fn without_cancellation_cases() -> Self {
        Self::with_copy_behavior(AsyncMemoryFault::None, false, false)
    }

    /// Creates a fixture whose provider completes copy through its native path.
    pub fn with_native_copy() -> Self {
        Self::with_copy_behavior(AsyncMemoryFault::None, false, true)
    }

    /// Creates a fixture without the core read, write, list, and copy
    /// capabilities.
    pub fn without_core_capabilities() -> Self {
        Self::with_configuration(
            AsyncMemoryFault::None,
            false,
            false,
            AsyncCapabilityProfile::NONE,
            "async-memory-contract-provider",
        )
    }

    /// Creates a fixture that advertises none of the suite operation
    /// capabilities.
    pub fn without_operation_capabilities() -> Self {
        Self::with_configuration(
            AsyncMemoryFault::None,
            false,
            false,
            AsyncCapabilityProfile::NONE,
            "async-memory-contract-provider",
        )
    }

    /// Creates an asynchronous fixture that exposes only core capabilities.
    pub fn without_optional_capabilities() -> Self {
        Self::with_configuration(
            AsyncMemoryFault::None,
            false,
            false,
            AsyncCapabilityProfile::CORE,
            "async-memory-contract-provider",
        )
    }

    /// Creates a conforming fixture whose filesystem and provider identifiers
    /// are identical.
    pub fn with_matching_ids() -> Self {
        Self::with_configuration(
            AsyncMemoryFault::None,
            false,
            false,
            AsyncCapabilityProfile::STANDARD,
            "async-memory-contract",
        )
    }

    /// Creates an asynchronous fixture using object and prefix metadata kinds.
    pub fn with_object_kinds() -> Self {
        Self::with_configuration(
            AsyncMemoryFault::ObjectKinds,
            false,
            false,
            AsyncCapabilityProfile::STANDARD,
            "async-memory-object-provider",
        )
    }

    /// Creates an asynchronous fixture supporting recursive prefix deletion
    /// without directory creation.
    pub fn recursive_delete_without_create_directory() -> Self {
        Self::with_configuration(
            AsyncMemoryFault::None,
            false,
            false,
            AsyncCapabilityProfile::PREFIX_DELETE,
            "async-memory-prefix-provider",
        )
    }

    /// Creates an asynchronous fixture advertising every capability contract.
    pub fn with_all_capabilities() -> Self {
        Self::with_configuration(
            AsyncMemoryFault::None,
            false,
            false,
            AsyncCapabilityProfile::ALL,
            "async-memory-all-capabilities-provider",
        )
    }

    /// Creates a fixture with selected copy and fault behavior.
    fn with_copy_behavior(
        fault: AsyncMemoryFault,
        supports_cancellation_cases: bool,
        native_copy: bool,
    ) -> Self {
        Self::with_configuration(
            fault,
            supports_cancellation_cases,
            native_copy,
            AsyncCapabilityProfile::STANDARD,
            "async-memory-contract-provider",
        )
    }

    /// Creates a fixture with selected capabilities and copy behavior.
    fn with_configuration(
        fault: AsyncMemoryFault,
        supports_cancellation_cases: bool,
        native_copy: bool,
        capabilities: AsyncCapabilityProfile,
        provider_id: &'static str,
    ) -> Self {
        let stage =
            Arc::new(Mutex::new(AsyncCopyCancellationStage::NativeAttempt));
        let entries = Arc::new(Mutex::new(HashMap::new()));
        let path_calls = Arc::new(AtomicUsize::new(0));
        let file_system = qubit_fs::AsyncFileSystem::from_spi(AsyncMemorySpi {
            stage: Arc::clone(&stage),
            entries: Arc::clone(&entries),
            fault,
            native_copy,
            core_capabilities: capabilities.core,
            optional_capabilities: capabilities.optional,
            create_directory_capability: capabilities.create_directory,
            extended_capabilities: capabilities.extended,
            provider_id,
        })
        .expect("async memory SPI properties must be valid");
        Self {
            file_system,
            stage,
            entries,
            supports_cancellation_cases,
            path_calls,
        }
    }

    /// Returns whether the fixture namespace contains no resources.
    pub fn is_empty(&self) -> bool {
        self.entries
            .lock()
            .expect("async memory state lock must succeed")
            .is_empty()
    }

    /// Returns how many contract paths the suite requested from this fixture.
    pub fn path_call_count(&self) -> usize {
        self.path_calls.load(Ordering::Relaxed)
    }
}

impl AsyncFileSystemFixture for AsyncMemoryFixture {
    fn file_system(&self) -> &qubit_fs::AsyncFileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.path_calls.fetch_add(1, Ordering::Relaxed);
        MemoryFixture::path_for(relative)
    }

    fn seed_file<'a>(
        &'a self,
        relative: &'a str,
        bytes: &'a [u8],
    ) -> FixtureFuture<'a, FixtureSupport<Path>> {
        Box::pin(async move {
            let path = self.path(relative)?;
            self.entries
                .lock()
                .expect("async memory state lock must succeed")
                .insert(path.as_str().to_owned(), Entry::File(bytes.to_vec()));
            Ok(FixtureSupport::Supported(path))
        })
    }

    fn read_file<'a>(
        &'a self,
        path: &'a Path,
    ) -> FixtureFuture<'a, FixtureSupport<Vec<u8>>> {
        Box::pin(async move {
            let entry = self
                .entries
                .lock()
                .expect("async memory state lock must succeed")
                .get(path.as_str())
                .cloned();
            Ok(match entry {
                Some(Entry::File(bytes)) => FixtureSupport::Supported(bytes),
                Some(Entry::Directory | Entry::Symlink) | None => {
                    FixtureSupport::Unsupported
                }
            })
        })
    }

    fn resource_version<'a>(
        &'a self,
        path: &'a Path,
    ) -> FixtureFuture<'a, FixtureSupport<qubit_fs::ResourceVersion>> {
        Box::pin(async move {
            let exists = self
                .entries
                .lock()
                .expect("async memory state lock must succeed")
                .contains_key(path.as_str());
            Ok(if exists {
                FixtureSupport::Supported(qubit_fs::ResourceVersion::new("v1"))
            } else {
                FixtureSupport::Unsupported
            })
        })
    }

    fn seed_empty_directory<'a>(
        &'a self,
        relative: &'a str,
    ) -> FixtureFuture<'a, FixtureSupport<Path>> {
        Box::pin(async move {
            let path = self.path(relative)?;
            self.entries
                .lock()
                .expect("async memory state lock must succeed")
                .insert(path.as_str().to_owned(), Entry::Directory);
            Ok(FixtureSupport::Supported(path))
        })
    }

    fn seed_symlink<'a>(
        &'a self,
        relative: &'a str,
    ) -> FixtureFuture<'a, FixtureSupport<Path>> {
        Box::pin(async move {
            let path = self.path(relative)?;
            self.entries
                .lock()
                .expect("async memory state lock must succeed")
                .insert(path.as_str().to_owned(), Entry::Symlink);
            Ok(FixtureSupport::Supported(path))
        })
    }

    fn copy_fast_path_case<'a>(
        &'a self,
        method: CopyMethod,
    ) -> FixtureFuture<'a, FixtureSupport<CopyFixtureCase>> {
        Box::pin(async move {
            if method != CopyMethod::ServerSide {
                return Ok(FixtureSupport::Unsupported);
            }
            let source = self.path("async-server-side-copy-source")?;
            let target = self.path("async-server-side-copy-target")?;
            self.entries
                .lock()
                .expect("async memory state lock must succeed")
                .insert(
                    source.as_str().to_owned(),
                    Entry::File(b"server-side".to_vec()),
                );
            Ok(FixtureSupport::Supported(CopyFixtureCase::new(
                source,
                target,
                qubit_fs::CopyOptions::default()
                    .with_server_side(ServerSidePreference::Require),
            )))
        })
    }

    fn copy_cancellation_case(
        &self,
        stage: AsyncCopyCancellationStage,
    ) -> FixtureResult<FixtureSupport<AsyncCopyFixtureCase>> {
        if !self.supports_cancellation_cases {
            return Ok(FixtureSupport::Unsupported);
        }
        *self.stage.lock().expect("async stage lock must succeed") = stage;
        let source = self.path("async-copy-source")?;
        self.entries
            .lock()
            .expect("async memory state lock must succeed")
            .insert(
                source.as_str().to_owned(),
                Entry::File(b"copy bytes".to_vec()),
            );
        Ok(FixtureSupport::Supported(AsyncCopyFixtureCase::new(
            source,
            self.path("async-copy-target")?,
            qubit_fs::CopyOptions::default(),
        )))
    }
}

struct AsyncMemorySpi {
    stage: Arc<Mutex<AsyncCopyCancellationStage>>,
    entries: Arc<Mutex<HashMap<String, Entry>>>,
    fault: AsyncMemoryFault,
    native_copy: bool,
    core_capabilities: bool,
    optional_capabilities: bool,
    create_directory_capability: bool,
    extended_capabilities: bool,
    provider_id: &'static str,
}

impl AsyncMemorySpi {
    /// Reads the currently selected pending stage.
    fn stage(&self) -> AsyncCopyCancellationStage {
        *self.stage.lock().expect("async stage lock must succeed")
    }

    /// Returns the fixed provider identity for a opened handle.
    fn info(path: &Path) -> OpenedFileInfo {
        OpenedFileInfo::new(
            FileSystemId::new("async-memory-contract")
                .expect("async provider id must be valid"),
            path.clone(),
        )
    }

    /// Returns the safe error used by unused methods.
    fn unused(operation: FsOperation) -> FsError {
        FsError::new(
            FsErrorKind::UnsupportedOperation,
            operation,
            "unused async memory SPI operation",
        )
    }
}

impl qubit_fs::spi::AsyncFileSystemSpi for AsyncMemorySpi {
    fn properties(&self) -> FileSystemProperties {
        let mut capabilities = FileSystemCapabilities::new();
        if self.optional_capabilities {
            capabilities = capabilities
                .with(FileSystemCapability::Delete)
                .with(FileSystemCapability::Rename)
                .with(FileSystemCapability::TempFile)
                .with(FileSystemCapability::TempDirectory)
                .with(FileSystemCapability::Append)
                .with(FileSystemCapability::RecursiveDelete)
                .with(FileSystemCapability::AtomicRename)
                .with(FileSystemCapability::AtomicReplace)
                .with(FileSystemCapability::DurableCopy)
                .with(FileSystemCapability::AtomicTempPersist)
                .with(FileSystemCapability::ServerSideCopy);
        }
        if self.create_directory_capability {
            capabilities =
                capabilities.with(FileSystemCapability::CreateDirectory);
        }
        if self.extended_capabilities {
            for capability in [
                FileSystemCapability::RangeRead,
                FileSystemCapability::ConditionalRead,
                FileSystemCapability::ChecksumValidation,
                FileSystemCapability::ConditionalWrite,
                FileSystemCapability::EmptyDirectory,
                FileSystemCapability::ConditionalDelete,
                FileSystemCapability::Symlink,
            ] {
                capabilities.insert(capability);
            }
        }
        if self.core_capabilities {
            capabilities = capabilities
                .with(FileSystemCapability::Copy)
                .with(FileSystemCapability::Read)
                .with(FileSystemCapability::Write)
                .with(FileSystemCapability::List);
        }
        FileSystemProperties::new(
            FileSystemInfo::new(
                FileSystemId::new("async-memory-contract")
                    .expect("async provider id must be valid"),
                self.provider_id,
                PathSemantics::Hierarchical,
            ),
            capabilities,
            FileSystemLimits::unknown(),
            PathConstraints::absolute(),
        )
        .expect("async memory properties must be valid")
    }

    fn stat<'a>(
        &'a self,
        request: StatRequest<'a>,
    ) -> qubit_fs::spi::SpiFuture<'a, FsResult<StatResponse>> {
        let path = request.path().clone();
        let entry = self
            .entries
            .lock()
            .expect("async memory state lock must succeed")
            .get(path.as_str())
            .cloned();
        let fault = self.fault;
        Box::pin(async move {
            match entry {
                Some(Entry::File(bytes)) => {
                    let mut metadata = FileMetadata::new(
                        if fault == AsyncMemoryFault::ObjectKinds {
                            FileKind::Object
                        } else {
                            FileKind::File
                        },
                    );
                    metadata = metadata.with_len(Some(bytes.len() as u64));
                    if fault == AsyncMemoryFault::WrongStatMetadata {
                        metadata = metadata
                            .with_kind(FileKind::Directory)
                            .with_len(None);
                    }
                    Ok(StatResponse::new(path, metadata))
                }
                Some(Entry::Directory) => Ok(StatResponse::new(
                    path,
                    FileMetadata::new(
                        if fault == AsyncMemoryFault::ObjectKinds {
                            FileKind::Prefix
                        } else {
                            FileKind::Directory
                        },
                    ),
                )),
                Some(Entry::Symlink) => Ok(StatResponse::new(
                    path,
                    FileMetadata::new(FileKind::Symlink),
                )),
                None if fault == AsyncMemoryFault::MissingPathExists => Ok(
                    StatResponse::new(path, FileMetadata::new(FileKind::File)),
                ),
                None => Err(FsError::new(
                    FsErrorKind::NotFound,
                    FsOperation::Stat,
                    "async memory entry absent",
                )),
            }
        })
    }

    fn list<'a>(
        &'a self,
        request: ListRequest<'a>,
    ) -> qubit_fs::spi::SpiFuture<
        'a,
        FsResult<qubit_fs::spi::OpenedAsyncDirectoryStream>,
    > {
        let entries = if self.fault == AsyncMemoryFault::ListEscapesNamespace {
            vec![qubit_fs::DirEntry::new(
                Path::parse("/outside-list-root")
                    .expect("fixed list entry path must be valid"),
                FileKind::Directory,
            )]
        } else if self.fault == AsyncMemoryFault::EmptyList {
            Vec::new()
        } else {
            listed_entries(
                &self
                    .entries
                    .lock()
                    .expect("async memory state lock must succeed"),
                request.path(),
                request.options().options(),
                self.fault != AsyncMemoryFault::ListDropsMetadata,
            )
        };
        Box::pin(async move {
            Ok(qubit_fs::spi::OpenedAsyncDirectoryStream::new(Box::new(
                AsyncMemoryDirectoryStream {
                    entries: entries.into_iter(),
                },
            )))
        })
    }

    fn open_reader<'a>(
        &'a self,
        request: OpenReaderRequest<'a>,
    ) -> qubit_fs::spi::SpiFuture<'a, FsResult<qubit_fs::spi::OpenedAsyncReader>>
    {
        if request.path().as_str() == "/contract/async-copy-source"
            && self.stage() == AsyncCopyCancellationStage::Reader
        {
            return Box::pin(future::pending());
        }
        let path = request.path().clone();
        let info = Self::info(&path);
        let bytes = self
            .entries
            .lock()
            .expect("async memory state lock must succeed")
            .get(path.as_str())
            .cloned();
        let fault = self.fault;
        let options = request.options().options().clone();
        Box::pin(async move {
            let Some(Entry::File(mut bytes)) = bytes else {
                return Err(FsError::new(
                    FsErrorKind::NotFound,
                    FsOperation::OpenReader,
                    "async memory entry absent",
                ));
            };
            if options
                .if_match()
                .as_ref()
                .is_some_and(|version| version.as_str() != "v1")
                || options
                    .if_none_match()
                    .as_ref()
                    .is_some_and(|version| version.as_str() == "v1")
            {
                return Err(FsError::new(
                    FsErrorKind::PreconditionFailed,
                    FsOperation::OpenReader,
                    "async memory read condition failed",
                ));
            }
            if fault == AsyncMemoryFault::ReadWrongBytes {
                bytes = b"wrong bytes".to_vec();
            }
            let start =
                options.offset().unwrap_or(0).min(bytes.len() as u64) as usize;
            let end = options.length().map_or(bytes.len(), |length| {
                start.saturating_add(length as usize).min(bytes.len())
            });
            bytes = bytes[start..end].to_vec();
            Ok(qubit_fs::spi::OpenedAsyncReader::new(
                info,
                Box::new(AsyncMemoryReader { bytes, offset: 0 }),
            ))
        })
    }

    fn open_writer<'a>(
        &'a self,
        request: OpenWriterRequest<'a>,
    ) -> qubit_fs::spi::SpiFuture<'a, FsResult<qubit_fs::spi::OpenedAsyncWriter>>
    {
        let stage = if request.path().as_str() == "/contract/async-copy-target"
        {
            self.stage()
        } else {
            AsyncCopyCancellationStage::NativeAttempt
        };
        let path = request.path().clone();
        let info = Self::info(&path);
        let state = Arc::clone(&self.entries);
        let fault = self.fault;
        let disposition = request.options().options().disposition();
        let atomicity = request.options().options().atomicity();
        let precondition = request.options().options().precondition().clone();
        Box::pin(async move {
            Ok(qubit_fs::spi::OpenedAsyncWriter::new(
                info,
                Box::new(AsyncMemoryWriter {
                    stage,
                    state,
                    path,
                    bytes: Vec::new(),
                    fault,
                    disposition,
                    atomicity,
                    precondition,
                }),
            ))
        })
    }

    fn create_directory<'a>(
        &'a self,
        request: CreateDirectoryRequest<'a>,
    ) -> qubit_fs::spi::SpiFuture<'a, FsResult<CreateDirectoryOutcome>> {
        let path = request.path().clone();
        let entries = Arc::clone(&self.entries);
        Box::pin(async move {
            let already_existed = entries
                .lock()
                .expect("async memory state lock must succeed")
                .insert(path.as_str().to_owned(), Entry::Directory)
                .is_some();
            Ok(CreateDirectoryOutcome::new(already_existed))
        })
    }

    fn delete_file<'a>(
        &'a self,
        request: DeleteFileRequest<'a>,
    ) -> qubit_fs::spi::SpiFuture<'a, FsResult<DeleteOutcome>> {
        let path = request.path().clone();
        let entries = Arc::clone(&self.entries);
        let fault = self.fault;
        Box::pin(async move {
            let missing = if fault == AsyncMemoryFault::DeleteNoOp {
                false
            } else {
                entries
                    .lock()
                    .expect("async memory state lock must succeed")
                    .remove(path.as_str())
                    .is_none()
            };
            Ok(DeleteOutcome::new(missing))
        })
    }

    fn delete_directory<'a>(
        &'a self,
        request: DeleteDirectoryRequest<'a>,
    ) -> qubit_fs::spi::SpiFuture<'a, FsResult<DeleteOutcome>> {
        let path = request.path().clone();
        let entries = Arc::clone(&self.entries);
        let recursive = request.options().options().recursive();
        let fault = self.fault;
        Box::pin(async move {
            let missing = if fault == AsyncMemoryFault::DeleteNoOp {
                true
            } else {
                let mut entries = entries
                    .lock()
                    .expect("async memory state lock must succeed");
                let removed = entries.remove(path.as_str());
                let mut removed_descendant = false;
                if recursive
                    && fault != AsyncMemoryFault::RecursiveDeleteLeavesChildren
                {
                    let prefix =
                        format!("{}/", path.as_str().trim_end_matches('/'));
                    let before = entries.len();
                    entries.retain(|entry_path, _| {
                        !entry_path.starts_with(&prefix)
                    });
                    removed_descendant = entries.len() != before;
                }
                removed.is_none() && !removed_descendant
            };
            Ok(DeleteOutcome::new(missing))
        })
    }

    fn try_copy<'a>(
        &'a self,
        request: qubit_fs::spi::CopyRequest<'a>,
    ) -> qubit_fs::spi::SpiFuture<
        'a,
        Result<CopyAttempt, qubit_fs::spi::SpiCopyFailure>,
    > {
        if request.source().as_str() == "/contract/async-copy-source"
            && self.stage() == AsyncCopyCancellationStage::NativeAttempt
        {
            return Box::pin(future::pending());
        }
        let options = request.options().options();
        let durable =
            options.durability() == qubit_fs::DurabilityRequirement::Required;
        let server_side = options.server_side() == ServerSidePreference::Require;
        let conflict = options.conflict();
        if self.native_copy
            || durable
            || server_side
            || options.mode() == CopyMode::Tree
            || conflict == qubit_fs::CopyConflictPolicy::Overwrite
        {
            let source = request.source().clone();
            let target = request.target().clone();
            let entries = Arc::clone(&self.entries);
            let fault = self.fault;
            return Box::pin(async move {
                let mut entries = entries
                    .lock()
                    .expect("async memory state lock must succeed");
                let Some(entry) = entries.get(source.as_str()).cloned() else {
                    return Err(qubit_fs::spi::SpiCopyFailure::new(
                        FsError::new(
                            FsErrorKind::NotFound,
                            FsOperation::Copy,
                            "async memory copy source absent",
                        ),
                        qubit_fs::CopyFailureState::Unchanged,
                        qubit_fs::CopyStats::default(),
                    ));
                };
                if entries.contains_key(target.as_str()) {
                    match conflict {
                        qubit_fs::CopyConflictPolicy::Fail => {
                            return Err(qubit_fs::spi::SpiCopyFailure::new(
                                FsError::new(
                                    FsErrorKind::AlreadyExists,
                                    FsOperation::Copy,
                                    "async memory copy target already exists",
                                ),
                                qubit_fs::CopyFailureState::Unchanged,
                                qubit_fs::CopyStats::default(),
                            ));
                        }
                        qubit_fs::CopyConflictPolicy::Skip => {
                            return Ok(CopyAttempt::Completed(
                                qubit_fs::CopyOutcome::new(
                                    qubit_fs::CopyStats {
                                        skipped: 1,
                                        ..qubit_fs::CopyStats::default()
                                    },
                                    qubit_fs::CopyMethod::Native,
                                    AchievedAtomicity::NonAtomic,
                                ),
                            ));
                        }
                        qubit_fs::CopyConflictPolicy::Overwrite => {}
                    }
                }
                let bytes = match &entry {
                    Entry::File(bytes) => bytes.len() as u64,
                    Entry::Directory | Entry::Symlink => 0,
                };
                let directory = matches!(&entry, Entry::Directory);
                let overwritten = entries.contains_key(target.as_str())
                    && conflict == qubit_fs::CopyConflictPolicy::Overwrite;
                entries.insert(target.as_str().to_owned(), entry);
                if directory
                    && fault != AsyncMemoryFault::DirectoryCopyDropsChildren
                {
                    let source_prefix =
                        format!("{}/", source.as_str().trim_end_matches('/'));
                    let target_prefix =
                        format!("{}/", target.as_str().trim_end_matches('/'));
                    let descendants = entries
                        .iter()
                        .filter_map(|(path, entry)| {
                            path.strip_prefix(&source_prefix).map(|relative| {
                                (
                                    format!("{target_prefix}{relative}"),
                                    entry.clone(),
                                )
                            })
                        })
                        .collect::<Vec<_>>();
                    for (path, entry) in descendants {
                        entries.insert(path, entry);
                    }
                }
                Ok(CopyAttempt::Completed(
                    qubit_fs::CopyOutcome::new(
                        qubit_fs::CopyStats {
                            files: 1,
                            bytes,
                            overwritten: u64::from(overwritten),
                            ..qubit_fs::CopyStats::default()
                        },
                        if server_side {
                            qubit_fs::CopyMethod::ServerSide
                        } else {
                            qubit_fs::CopyMethod::Native
                        },
                        AchievedAtomicity::NonAtomic,
                    )
                    .with_durable(
                        durable
                            && fault != AsyncMemoryFault::DurableCopyNonDurable,
                    ),
                ))
            });
        }
        Box::pin(async {
            Ok(CopyAttempt::Declined(
                qubit_fs::spi::CopyDeclineReason::NotApplicable,
            ))
        })
    }

    fn rename<'a>(
        &'a self,
        request: RenameRequest<'a>,
    ) -> qubit_fs::spi::SpiFuture<'a, Result<RenameOutcome, SpiRenameFailure>>
    {
        let source = request.source().clone();
        let target = request.target().clone();
        let entries = Arc::clone(&self.entries);
        let fault = self.fault;
        let atomicity = request.options().options().atomicity();
        let overwrite = request.options().options().overwrite();
        Box::pin(async move {
            let mut entries = entries
                .lock()
                .expect("async memory state lock must succeed");
            if !overwrite && entries.contains_key(target.as_str()) {
                return Err(SpiRenameFailure::new(
                    FsError::new(
                        FsErrorKind::AlreadyExists,
                        FsOperation::Rename,
                        "async memory rename target already exists",
                    ),
                    RenameFailureState::Unchanged,
                ));
            }
            if fault != AsyncMemoryFault::RenameNoOp {
                let Some(entry) = entries.remove(source.as_str()) else {
                    return Err(SpiRenameFailure::new(
                        FsError::new(
                            FsErrorKind::NotFound,
                            FsOperation::Rename,
                            "async memory entry absent",
                        ),
                        RenameFailureState::Unchanged,
                    ));
                };
                entries.insert(target.as_str().to_owned(), entry);
            }
            drop(entries);
            let (reported_source, reported_target) =
                if fault == AsyncMemoryFault::RenameWrongOutcome {
                    (
                        Path::parse("/contract/async-wrong-rename-source")
                            .expect("generated path must be valid"),
                        Path::parse("/contract/async-wrong-rename-target")
                            .expect("generated path must be valid"),
                    )
                } else {
                    (source, target)
                };
            Ok(RenameOutcome::new(
                reported_source,
                reported_target,
                if atomicity == AtomicityRequirement::Required
                    && fault != AsyncMemoryFault::AtomicRenameNonAtomic
                {
                    AchievedAtomicity::Atomic
                } else {
                    AchievedAtomicity::NonAtomic
                },
                PublicationMethod::Direct,
            ))
        })
    }

    fn create_temp_file<'a>(
        &'a self,
        request: CreateTempFileRequest,
    ) -> qubit_fs::spi::SpiFuture<
        'a,
        FsResult<qubit_fs::spi::OpenedAsyncTempFile>,
    > {
        let entries = Arc::clone(&self.entries);
        let fault = self.fault;
        let options = request.options().clone();
        Box::pin(async move {
            let path = allocate_async_temp(
                &entries,
                false,
                options.parent(),
                options.prefix(),
                options.suffix(),
                fault,
            );
            Ok(qubit_fs::spi::OpenedAsyncTempFile::new(
                Self::info(&path)
                    .with_metadata(FileMetadata::new(FileKind::File)),
                Box::new(AsyncTempSession {
                    entries,
                    path,
                    fault,
                }),
            ))
        })
    }

    fn create_temp_directory<'a>(
        &'a self,
        request: CreateTempDirectoryRequest,
    ) -> qubit_fs::spi::SpiFuture<
        'a,
        FsResult<qubit_fs::spi::OpenedAsyncTempDirectory>,
    > {
        let entries = Arc::clone(&self.entries);
        let fault = self.fault;
        let options = request.options().clone();
        Box::pin(async move {
            let path = allocate_async_temp(
                &entries,
                true,
                options.parent(),
                options.prefix(),
                options.suffix(),
                fault,
            );
            Ok(qubit_fs::spi::OpenedAsyncTempDirectory::new(
                Self::info(&path)
                    .with_metadata(FileMetadata::new(FileKind::Directory)),
                Box::new(AsyncTempSession {
                    entries,
                    path,
                    fault,
                }),
            ))
        })
    }
}

struct AsyncMemoryDirectoryStream {
    entries: std::vec::IntoIter<qubit_fs::DirEntry>,
}

impl qubit_fs::spi::AsyncDirectoryStreamSession for AsyncMemoryDirectoryStream {
    fn next_entry_async<'a>(
        &'a mut self,
    ) -> qubit_fs::spi::SpiFuture<'a, FsResult<Option<qubit_fs::DirEntry>>>
    {
        Box::pin(async move { Ok(self.entries.next()) })
    }
}

struct AsyncMemoryReader {
    bytes: Vec<u8>,
    offset: usize,
}

impl AsyncInput for AsyncMemoryReader {
    type Item = u8;

    unsafe fn poll_read_unchecked(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        output: &mut [u8],
        index: usize,
        count: usize,
    ) -> Poll<IoResult<usize>> {
        let this = self.get_mut();
        if count == 0 || this.offset == this.bytes.len() {
            Poll::Ready(Ok(0))
        } else {
            let length = count.min(this.bytes.len() - this.offset);
            output[index..index + length].copy_from_slice(
                &this.bytes[this.offset..this.offset + length],
            );
            this.offset += length;
            Poll::Ready(Ok(length))
        }
    }
}

struct AsyncMemoryWriter {
    stage: AsyncCopyCancellationStage,
    state: Arc<Mutex<HashMap<String, Entry>>>,
    path: Path,
    bytes: Vec<u8>,
    fault: AsyncMemoryFault,
    disposition: WriteDisposition,
    atomicity: AtomicityRequirement,
    precondition: WritePrecondition,
}

impl AsyncOutput for AsyncMemoryWriter {
    type Item = u8;

    unsafe fn poll_write_unchecked(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        input: &[u8],
        index: usize,
        count: usize,
    ) -> Poll<IoResult<usize>> {
        let this = self.get_mut();
        if this.stage == AsyncCopyCancellationStage::Writer {
            Poll::Pending
        } else {
            this.bytes.extend_from_slice(&input[index..index + count]);
            Poll::Ready(Ok(count))
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<IoResult<()>> {
        let _ = self;
        Poll::Ready(Ok(()))
    }
}

impl qubit_fs::spi::AsyncFileWriteSession for AsyncMemoryWriter {
    fn commit_async<'a>(
        self: Pin<&'a mut Self>,
    ) -> qubit_fs::spi::SpiFuture<
        'a,
        Result<qubit_fs::WriteOutcome, qubit_fs::WriteFailure>,
    > {
        if self.as_ref().get_ref().stage == AsyncCopyCancellationStage::Commit {
            return Box::pin(future::pending());
        }
        let this = self.get_mut();
        let state = Arc::clone(&this.state);
        let path = this.path.clone();
        let bytes = this.bytes.clone();
        let fault = this.fault;
        let disposition = this.disposition;
        let atomicity = this.atomicity;
        let precondition = this.precondition.clone();
        Box::pin(async move {
            let destination_exists = state
                .lock()
                .expect("async memory state lock must succeed")
                .contains_key(path.as_str());
            if disposition == WriteDisposition::CreateNew && destination_exists
            {
                return Err(qubit_fs::WriteFailure::new(
                    FsError::new(
                        FsErrorKind::AlreadyExists,
                        FsOperation::CommitWriter,
                        "async memory destination already exists",
                    ),
                    WriteFailureState::NotPublished,
                ));
            }
            if precondition == WritePrecondition::IfAbsent && destination_exists
            {
                return Err(qubit_fs::WriteFailure::new(
                    FsError::new(
                        FsErrorKind::PreconditionFailed,
                        FsOperation::CommitWriter,
                        "async memory destination violates if-absent",
                    ),
                    WriteFailureState::NotPublished,
                ));
            }
            if fault != AsyncMemoryFault::WriteDropsBytes
                && !(fault == AsyncMemoryFault::CopyDropsTarget
                    && path.as_str().contains("async-copy-positive-target"))
            {
                let mut state =
                    state.lock().expect("async memory state lock must succeed");
                let bytes = if disposition == WriteDisposition::Append
                    && fault != AsyncMemoryFault::AppendOverwrites
                {
                    let mut combined = match state.get(path.as_str()) {
                        Some(Entry::File(existing)) => existing.clone(),
                        Some(Entry::Directory | Entry::Symlink) | None => {
                            Vec::new()
                        }
                    };
                    combined.extend_from_slice(&bytes);
                    combined
                } else {
                    bytes
                };
                state.insert(path.as_str().to_owned(), Entry::File(bytes));
            }
            Ok(qubit_fs::WriteOutcome::new(
                if atomicity == AtomicityRequirement::Required
                    && fault != AsyncMemoryFault::AtomicReplaceNonAtomic
                {
                    AchievedAtomicity::Atomic
                } else {
                    AchievedAtomicity::NonAtomic
                },
                PublicationMethod::Direct,
            ))
        })
    }

    fn abort_async<'a>(
        self: Pin<&'a mut Self>,
    ) -> qubit_fs::spi::SpiFuture<'a, FsResult<()>> {
        let _ = self;
        Box::pin(async { Ok(()) })
    }

    fn cancel_on_drop(self: Pin<&mut Self>) {
        let _ = self;
    }
}

/// Allocates an isolated path and inserts its temporary resource entry.
///
/// `entries` owns the fixture namespace and `directory` selects the resource
/// kind. The returned path is already present in that namespace.
fn allocate_async_temp(
    entries: &Arc<Mutex<HashMap<String, Entry>>>,
    directory: bool,
    parent: Option<&Path>,
    prefix: &str,
    suffix: &str,
    fault: AsyncMemoryFault,
) -> Path {
    let mut entries = entries
        .lock()
        .expect("async memory state lock must succeed");
    let parent = if fault == AsyncMemoryFault::TempIgnoresOptions {
        "/contract"
    } else {
        parent.map_or("/contract", Path::as_str)
    };
    let (prefix, suffix) = if fault == AsyncMemoryFault::TempIgnoresOptions {
        (".async-tmp-", "")
    } else {
        (prefix, suffix)
    };
    let separator = if parent == "/" { "" } else { "/" };
    let path = Path::parse(&format!(
        "{parent}{separator}{prefix}{}{suffix}",
        entries.len()
    ))
    .expect("generated temporary path must be valid");
    entries.insert(
        path.as_str().to_owned(),
        if directory {
            Entry::Directory
        } else {
            Entry::File(Vec::new())
        },
    );
    path
}

/// Minimal asynchronous temporary-resource session for suite self-tests.
///
/// The session mutates the shared fixture namespace and applies its configured
/// fault when cleanup is requested.
struct AsyncTempSession {
    entries: Arc<Mutex<HashMap<String, Entry>>>,
    path: Path,
    fault: AsyncMemoryFault,
}

impl qubit_fs::spi::AsyncTempResourceSpi for AsyncTempSession {
    fn cleanup<'a>(
        self: Pin<&'a mut Self>,
    ) -> qubit_fs::spi::SpiFuture<'a, FsResult<()>> {
        let this = self.get_mut();
        let entries = Arc::clone(&this.entries);
        let path = this.path.clone();
        let fault = this.fault;
        Box::pin(async move {
            if fault != AsyncMemoryFault::TempCleanupNoOp {
                entries
                    .lock()
                    .expect("async memory state lock must succeed")
                    .remove(path.as_str());
            }
            Ok(())
        })
    }

    fn keep<'a>(
        self: Pin<&'a mut Self>,
    ) -> qubit_fs::spi::SpiFuture<'a, FsResult<()>> {
        let _ = self;
        Box::pin(async { Ok(()) })
    }

    fn persist<'a>(
        self: Pin<&'a mut Self>,
        request: qubit_fs::spi::PersistRequest<'a>,
    ) -> qubit_fs::spi::SpiFuture<
        'a,
        Result<PersistOutcome, qubit_fs::spi::SpiPersistFailure>,
    > {
        let this = self.get_mut();
        let entries = Arc::clone(&this.entries);
        let source = this.path.clone();
        let target = request.target().clone();
        let atomicity = request.options().atomicity();
        let fault = this.fault;
        Box::pin(async move {
            let mut entries = entries
                .lock()
                .expect("async memory state lock must succeed");
            let entry = entries
                .remove(source.as_str())
                .expect("temporary entry must exist");
            entries.insert(target.as_str().to_owned(), entry);
            let reported_target =
                if fault == AsyncMemoryFault::TempPersistWrongTarget {
                    Path::parse("/contract/async-wrong-persist-target")
                        .expect("generated path must be valid")
                } else {
                    target
                };
            Ok(PersistOutcome::new(
                reported_target,
                if atomicity == AtomicityRequirement::Required
                    && fault != AsyncMemoryFault::AtomicTempPersistNonAtomic
                {
                    AchievedAtomicity::Atomic
                } else {
                    AchievedAtomicity::NonAtomic
                },
                PublicationMethod::Direct,
            ))
        })
    }
}
