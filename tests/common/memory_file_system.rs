// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
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
    WriteOutcome,
};
use qubit_fs_testkit::{
    AsyncCopyCancellationStage,
    AsyncCopyFixtureCase,
    AsyncFileSystemFixture,
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
}

#[derive(Clone)]
enum Entry {
    File(Vec<u8>),
    Directory,
}

struct State {
    entries: HashMap<String, Entry>,
    next_temp: u64,
    fault: MemoryFault,
    delete_capability: bool,
    core_capabilities: bool,
}

impl MemoryFixture {
    /// Creates a fresh conforming fixture.
    pub fn new() -> Self {
        Self::with_fault(MemoryFault::None)
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

    /// Creates a conforming fixture whose filesystem and provider identifiers
    /// are identical.
    pub fn with_matching_ids() -> Self {
        Self::with_configuration(
            MemoryFault::None,
            true,
            true,
            "memory-contract",
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
            "memory-contract-provider",
        )
    }

    /// Creates a fixture with selected capabilities and provider identity.
    fn with_configuration(
        fault: MemoryFault,
        delete_capability: bool,
        core_capabilities: bool,
        provider_id: &'static str,
    ) -> Self {
        let state = Arc::new(Mutex::new(State {
            entries: HashMap::new(),
            next_temp: 0,
            fault,
            delete_capability,
            core_capabilities,
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

    fn list_prefix(&self, _: &Path, relative: &str) -> FixtureResult<String> {
        Ok(relative.to_owned())
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
            Some(Entry::Directory) | None => FixtureSupport::Unsupported,
        })
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
    fn create_temp(&self, directory: bool) -> Path {
        let mut state =
            self.state.lock().expect("memory state lock must succeed");
        let path = Path::parse(&format!("/contract/.tmp-{}", state.next_temp))
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
        let mut capabilities = FileSystemCapabilities::new()
            .with(FileSystemCapability::CreateDirectory)
            .with(FileSystemCapability::Rename)
            .with(FileSystemCapability::TempFile)
            .with(FileSystemCapability::TempDirectory);
        if state.core_capabilities {
            capabilities = capabilities
                .with(FileSystemCapability::Read)
                .with(FileSystemCapability::Write)
                .with(FileSystemCapability::List)
                .with(FileSystemCapability::Copy);
        }
        if state.delete_capability {
            capabilities = capabilities.with(FileSystemCapability::Delete);
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
            (_, Entry::File(_)) => FileKind::File,
            (_, Entry::Directory) => FileKind::Directory,
        };
        let mut metadata = FileMetadata::new(kind);
        if let Entry::File(bytes) = entry {
            metadata.len = Some(bytes.len() as u64);
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
        Ok(OpenedReader::new(
            Self::info(request.path().clone()),
            Box::new(Cursor::new(bytes.clone())),
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
        let removed = self
            .state
            .lock()
            .expect("memory state lock must succeed")
            .entries
            .remove(request.path().as_str());
        Ok(DeleteOutcome::new(removed.is_none()))
    }

    fn delete_directory(
        &self,
        request: DeleteDirectoryRequest<'_>,
    ) -> FsResult<DeleteOutcome> {
        let removed = self
            .state
            .lock()
            .expect("memory state lock must succeed")
            .entries
            .remove(request.path().as_str());
        Ok(DeleteOutcome::new(removed.is_none()))
    }

    fn try_copy(
        &self,
        _: qubit_fs::spi::CopyRequest<'_>,
    ) -> Result<CopyAttempt, qubit_fs::spi::SpiCopyFailure> {
        Ok(CopyAttempt::Declined(CopyDeclineReason::NotImplemented))
    }

    fn rename(
        &self,
        request: RenameRequest<'_>,
    ) -> Result<RenameOutcome, SpiRenameFailure> {
        let mut state =
            self.state.lock().expect("memory state lock must succeed");
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
        state
            .entries
            .insert(request.target().as_str().to_owned(), entry);
        Ok(RenameOutcome::new(
            request.source().clone(),
            request.target().clone(),
            AchievedAtomicity::NonAtomic,
            PublicationMethod::Direct,
        ))
    }

    fn create_temp_file(
        &self,
        _: CreateTempFileRequest,
    ) -> FsResult<OpenedTempFile> {
        let path = self.create_temp(false);
        Ok(OpenedTempFile::new(
            Self::info(path.clone()),
            Box::new(TempSession {
                state: Arc::clone(&self.state),
                path,
            }),
        ))
    }

    fn create_temp_directory(
        &self,
        _: CreateTempDirectoryRequest,
    ) -> FsResult<OpenedTempDirectory> {
        let path = self.create_temp(true);
        Ok(OpenedTempDirectory::new(
            Self::info(path.clone()),
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
) -> Vec<qubit_fs::DirEntry> {
    let prefix = format!("{}/", root.as_str().trim_end_matches('/'));
    entries
        .iter()
        .filter_map(|(text, entry)| {
            let relative = text.strip_prefix(&prefix)?;
            if relative.is_empty()
                || (!options.recursive
                    && options.prefix.is_none()
                    && relative.contains('/'))
            {
                return None;
            }
            if !options.prefix.as_deref().is_none_or(|prefix| {
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
            };
            Some(qubit_fs::DirEntry::new(
                Path::parse(text)
                    .expect("stored memory path must remain valid"),
                kind,
            ))
        })
        .collect()
}

struct MemoryWriter {
    state: Arc<Mutex<State>>,
    path: Path,
    bytes: Vec<u8>,
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
        self.state
            .lock()
            .expect("memory state lock must succeed")
            .entries
            .insert(
                self.path.as_str().to_owned(),
                Entry::File(self.bytes.clone()),
            );
        Ok(WriteOutcome::new(
            AchievedAtomicity::NonAtomic,
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
            AchievedAtomicity::NonAtomic,
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
    /// Returns bytes different from the provider's seeded content.
    ReadWrongBytes,
    /// Accepts writes but does not publish their bytes.
    WriteDropsBytes,
    /// Produces a listing entry outside the requested namespace.
    ListEscapesNamespace,
    /// Returns no entries for a non-empty requested directory.
    EmptyList,
    /// Reports deletion success without removing the resource.
    DeleteNoOp,
    /// Reports copy success without publishing the target bytes.
    CopyDropsTarget,
    /// Reports rename success without moving the resource.
    RenameNoOp,
    /// Reports temporary cleanup success without removing the resource.
    TempCleanupNoOp,
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
            false,
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
            true,
            "async-memory-contract",
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
            true,
            "async-memory-contract-provider",
        )
    }

    /// Creates a fixture with selected capabilities and copy behavior.
    fn with_configuration(
        fault: AsyncMemoryFault,
        supports_cancellation_cases: bool,
        native_copy: bool,
        core_capabilities: bool,
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
            core_capabilities,
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

    fn list_prefix(&self, _: &Path, relative: &str) -> FixtureResult<String> {
        Ok(relative.to_owned())
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
                Some(Entry::Directory) | None => FixtureSupport::Unsupported,
            })
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
        let mut capabilities = FileSystemCapabilities::new()
            .with(FileSystemCapability::Delete)
            .with(FileSystemCapability::Rename)
            .with(FileSystemCapability::TempFile)
            .with(FileSystemCapability::TempDirectory);
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
                    let mut metadata = FileMetadata::new(FileKind::File);
                    metadata.len = Some(bytes.len() as u64);
                    Ok(StatResponse::new(path, metadata))
                }
                Some(Entry::Directory) => Ok(StatResponse::new(
                    path,
                    FileMetadata::new(FileKind::Directory),
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
        Box::pin(async move {
            let Some(Entry::File(mut bytes)) = bytes else {
                return Err(FsError::new(
                    FsErrorKind::NotFound,
                    FsOperation::OpenReader,
                    "async memory entry absent",
                ));
            };
            if fault == AsyncMemoryFault::ReadWrongBytes {
                bytes = b"wrong bytes".to_vec();
            }
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
        Box::pin(async move {
            Ok(qubit_fs::spi::OpenedAsyncWriter::new(
                info,
                Box::new(AsyncMemoryWriter {
                    stage,
                    state,
                    path,
                    bytes: Vec::new(),
                    fault,
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
        Box::pin(async move {
            let missing = entries
                .lock()
                .expect("async memory state lock must succeed")
                .remove(path.as_str())
                .is_none();
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
        if self.native_copy {
            let source = request.source().clone();
            let target = request.target().clone();
            let entries = Arc::clone(&self.entries);
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
                let bytes = match &entry {
                    Entry::File(bytes) => bytes.len() as u64,
                    Entry::Directory => 0,
                };
                entries.insert(target.as_str().to_owned(), entry);
                Ok(CopyAttempt::Completed(qubit_fs::CopyOutcome::new(
                    qubit_fs::CopyStats {
                        files: 1,
                        bytes,
                        ..qubit_fs::CopyStats::default()
                    },
                    qubit_fs::CopyMethod::Native,
                    AchievedAtomicity::NonAtomic,
                )))
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
        Box::pin(async move {
            if fault != AsyncMemoryFault::RenameNoOp {
                let mut entries = entries
                    .lock()
                    .expect("async memory state lock must succeed");
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
            Ok(RenameOutcome::new(
                source,
                target,
                AchievedAtomicity::NonAtomic,
                PublicationMethod::Direct,
            ))
        })
    }

    fn create_temp_file<'a>(
        &'a self,
        _: CreateTempFileRequest,
    ) -> qubit_fs::spi::SpiFuture<
        'a,
        FsResult<qubit_fs::spi::OpenedAsyncTempFile>,
    > {
        let entries = Arc::clone(&self.entries);
        let fault = self.fault;
        Box::pin(async move {
            let path = allocate_async_temp(&entries, false);
            Ok(qubit_fs::spi::OpenedAsyncTempFile::new(
                Self::info(&path),
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
        _: CreateTempDirectoryRequest,
    ) -> qubit_fs::spi::SpiFuture<
        'a,
        FsResult<qubit_fs::spi::OpenedAsyncTempDirectory>,
    > {
        let entries = Arc::clone(&self.entries);
        let fault = self.fault;
        Box::pin(async move {
            let path = allocate_async_temp(&entries, true);
            Ok(qubit_fs::spi::OpenedAsyncTempDirectory::new(
                Self::info(&path),
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
        Box::pin(async move {
            if fault != AsyncMemoryFault::WriteDropsBytes
                && !(fault == AsyncMemoryFault::CopyDropsTarget
                    && path.as_str().contains("async-copy-positive-target"))
            {
                state
                    .lock()
                    .expect("async memory state lock must succeed")
                    .insert(path.as_str().to_owned(), Entry::File(bytes));
            }
            Ok(qubit_fs::WriteOutcome::new(
                AchievedAtomicity::NonAtomic,
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
) -> Path {
    let mut entries = entries
        .lock()
        .expect("async memory state lock must succeed");
    let path = Path::parse(&format!("/contract/.async-tmp-{}", entries.len()))
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
        Box::pin(async move {
            let mut entries = entries
                .lock()
                .expect("async memory state lock must succeed");
            let entry = entries
                .remove(source.as_str())
                .expect("temporary entry must exist");
            entries.insert(target.as_str().to_owned(), entry);
            Ok(PersistOutcome::new(
                target,
                AchievedAtomicity::NonAtomic,
                PublicationMethod::Direct,
            ))
        })
    }
}
