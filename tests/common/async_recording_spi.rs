// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Controllable asynchronous SPI used only by facade-level behavior tests.

use std::io::Error as IoError;
use std::io::Result as IoResult;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Context;
use std::task::Poll;

use qubit_fs::AsyncFileSystem;
use qubit_fs::FsError;
use qubit_fs::FsResult;
use qubit_fs::Path;
use qubit_fs::copy::CopyFailureState;
use qubit_fs::copy::CopyMethod;
use qubit_fs::copy::CopyOutcome;
use qubit_fs::copy::CopyStats;
use qubit_fs::directory::CreateDirectoryOutcome;
use qubit_fs::directory::DeleteOutcome;
use qubit_fs::error::FsEffectState;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::AchievedAtomicity;
use qubit_fs::metadata::DirEntry;
use qubit_fs::metadata::FileKind;
use qubit_fs::metadata::FileMetadata;
use qubit_fs::metadata::FileSystemCapabilities;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::FileSystemId;
use qubit_fs::metadata::FileSystemInfo;
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs::metadata::OpenedFileInfo;
use qubit_fs::metadata::PublicationMethod;
use qubit_fs::metadata::SymlinkPolicy;
use qubit_fs::metadata::WriteOutcome;
use qubit_fs::path::PathConstraints;
use qubit_fs::path::PathSemantics;
use qubit_fs::rename::RenameFailureState;
use qubit_fs::rename::RenameOutcome;
use qubit_fs::spi::AsyncDirectoryStreamSession;
use qubit_fs::spi::AsyncFileSystemSpi;
use qubit_fs::spi::AsyncFileWriteSession;
use qubit_fs::spi::AsyncTempResourceSpi;
use qubit_fs::spi::CopyAttempt;
use qubit_fs::spi::CopyDeclineReason;
use qubit_fs::spi::CopyRequest;
use qubit_fs::spi::CreateDirectoryRequest;
use qubit_fs::spi::CreateTempDirectoryRequest;
use qubit_fs::spi::CreateTempFileRequest;
use qubit_fs::spi::DeleteDirectoryRequest;
use qubit_fs::spi::DeleteFileRequest;
use qubit_fs::spi::ListRequest;
use qubit_fs::spi::OpenReaderRequest;
use qubit_fs::spi::OpenWriterRequest;
use qubit_fs::spi::OpenedAsyncDirectoryStream;
use qubit_fs::spi::OpenedAsyncReader;
use qubit_fs::spi::OpenedAsyncTempDirectory;
use qubit_fs::spi::OpenedAsyncTempFile;
use qubit_fs::spi::OpenedAsyncWriter;
use qubit_fs::spi::PersistRequest;
use qubit_fs::spi::ProviderOperation;
use qubit_fs::spi::ProviderOperations;
use qubit_fs::spi::ProviderProperties;
use qubit_fs::spi::RenameRequest;
use qubit_fs::spi::SpiCopyFailure;
use qubit_fs::spi::SpiFuture;
use qubit_fs::spi::SpiPersistFailure;
use qubit_fs::spi::SpiRenameFailure;
use qubit_fs::spi::StatRequest;
use qubit_fs::spi::StatResponse;
use qubit_fs::temp::PersistFailureState;
use qubit_fs::temp::PersistOutcome;
use qubit_fs::write::WriteAbortOutcome;
use qubit_fs::write::WriteFailure;
use qubit_fs::write::WriteFailureState;
use qubit_fs::write::WriteOptions;
use qubit_io::AsyncInput;
use qubit_io::AsyncOutput;

/// A fallback await point that can remain pending or return a deterministic
/// failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AsyncCopyStage {
    Stat,
    OpenReader,
    OpenWriter,
    TryCopy,
    ReaderRead,
    WriterWrite,
    WriterFlush,
    WriterCommit,
}

/// Controls the recording provider's externally observable behavior.
#[derive(Clone, Debug, Default)]
pub(crate) struct AsyncRecordingConfig {
    pub(crate) omitted_capability: Option<FileSystemCapability>,
    pub(crate) omit_read_and_write: bool,
    pub(crate) pending_stage: Option<AsyncCopyStage>,
    pub(crate) failing_stage: Option<AsyncCopyStage>,
    pub(crate) invalid_temp_identity: bool,
    pub(crate) invalid_temp_path: bool,
    pub(crate) invalid_temp_kind: bool,
    pub(crate) invalid_opened_identity: bool,
    pub(crate) invalid_stat_path: bool,
    pub(crate) stat_kind: Option<FileKind>,
    pub(crate) atomic_temp_persist: bool,
    pub(crate) completed_copy: Option<AchievedAtomicity>,
    pub(crate) decline_copy: bool,
    pub(crate) server_side_copy: bool,
    pub(crate) copy_failure: bool,
    pub(crate) rename_atomicity: Option<AchievedAtomicity>,
    pub(crate) rename_copy_then_delete: bool,
    pub(crate) temp_persist_indeterminate: bool,
    pub(crate) temp_persist_failure: Option<PersistFailureState>,
    pub(crate) temp_cleanup_failure: bool,
    pub(crate) temp_cleanup_effect: Option<FsEffectState>,
    pub(crate) temp_pending_operation: Option<FsOperation>,
    pub(crate) temp_keep_failure: bool,
    pub(crate) temp_create_error: bool,
    pub(crate) writer_atomicity: Option<AchievedAtomicity>,
    pub(crate) writer_durable: bool,
    pub(crate) writer_commit_failure: Option<WriteFailureState>,
    pub(crate) writer_abort_failure: Option<FsErrorKind>,
    pub(crate) writer_open_error: Option<FsErrorKind>,
    pub(crate) writer_open_effect: Option<FsEffectState>,
    /// Maximum bytes acknowledged by one successful provider write.
    pub(crate) writer_chunk: Option<usize>,
    /// Successful bytes before an injected write suspension or failure.
    pub(crate) writer_progress_before_stop: usize,
    pub(crate) range_read: bool,
    pub(crate) maximum_read_range_bytes: Option<u64>,
    pub(crate) maximum_write_bytes: Option<u64>,
    pub(crate) temp_persist_atomicity: Option<AchievedAtomicity>,
    pub(crate) directory_entries: Vec<DirEntry>,
    pub(crate) directory_error: bool,
    pub(crate) list_open_error: bool,
    pub(crate) create_directory_already_existed: bool,
    pub(crate) delete_already_missing: bool,
    pub(crate) create_directory_error: bool,
    pub(crate) delete_error: bool,
    pub(crate) rename_error: bool,
}

/// Exposes ordered provider call facts without leaking session internals.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct AsyncRecordingProbe {
    calls: Arc<Mutex<Vec<&'static str>>>,
    writer_cancellations: Arc<Mutex<usize>>,
    temp_cancellations: Arc<Mutex<usize>>,
    writer_options: Arc<Mutex<Vec<WriteOptions>>>,
}
impl AsyncRecordingProbe {
    /// Returns the calls observed so far.
    #[allow(dead_code)]
    pub(crate) fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().expect("calls lock should succeed").clone()
    }
    /// Returns local writer cancellation notifications.
    #[allow(dead_code)]
    pub(crate) fn writer_cancellations(&self) -> usize {
        *self
            .writer_cancellations
            .lock()
            .expect("cancellation lock should succeed")
    }
    /// Returns local temporary-resource cancellation notifications.
    #[allow(dead_code)]
    pub(crate) fn temp_cancellations(&self) -> usize {
        *self
            .temp_cancellations
            .lock()
            .expect("temporary cancellation lock should succeed")
    }
    /// Returns writer options observed by the provider.
    #[allow(dead_code)]
    pub(crate) fn writer_options(&self) -> Vec<WriteOptions> {
        self.writer_options
            .lock()
            .expect("writer options lock should succeed")
            .clone()
    }
}

/// Creates an async facade with controllable fallback and temporary sessions.
pub fn async_recording_file_system(config: AsyncRecordingConfig) -> (AsyncFileSystem, AsyncRecordingProbe) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let cancellations = Arc::new(Mutex::new(0));
    let temp_cancellations = Arc::new(Mutex::new(0));
    let writer_options = Arc::new(Mutex::new(Vec::new()));
    let probe = AsyncRecordingProbe {
        calls: Arc::clone(&calls),
        writer_cancellations: Arc::clone(&cancellations),
        temp_cancellations: Arc::clone(&temp_cancellations),
        writer_options: Arc::clone(&writer_options),
    };
    let file_system = AsyncFileSystem::from_spi(AsyncRecordingSpi {
        config,
        calls,
        cancellations,
        temp_cancellations,
        writer_options,
    })
    .expect("recording async facade should construct");
    (file_system, probe)
}

/// Records facade calls and supplies predictable fallback handles.
struct AsyncRecordingSpi {
    config: AsyncRecordingConfig,
    calls: Arc<Mutex<Vec<&'static str>>>,
    cancellations: Arc<Mutex<usize>>,
    temp_cancellations: Arc<Mutex<usize>>,
    writer_options: Arc<Mutex<Vec<WriteOptions>>>,
}
impl AsyncRecordingSpi {
    /// Records an SPI invocation.
    fn record(&self, call: &'static str) {
        self.calls.lock().expect("calls lock should succeed").push(call);
    }
    /// Builds the property snapshot used by tests.
    fn properties_for(&self) -> ProviderProperties {
        let mut capabilities = FileSystemCapabilities::new();
        for capability in [
            FileSystemCapability::Copy,
            FileSystemCapability::Read,
            FileSystemCapability::Write,
            FileSystemCapability::TempFile,
            FileSystemCapability::TempDirectory,
            FileSystemCapability::List,
            FileSystemCapability::CreateDirectory,
            FileSystemCapability::Delete,
        ] {
            let omitted = self.config.omitted_capability == Some(capability)
                || (self.config.omit_read_and_write
                    && matches!(capability, FileSystemCapability::Read | FileSystemCapability::Write));
            if !omitted {
                capabilities = capabilities.with_guaranteed(capability);
            }
        }
        if self.config.completed_copy.is_some() {
            capabilities = capabilities
                .with_guaranteed(FileSystemCapability::AtomicReplace)
                .with_guaranteed(FileSystemCapability::AtomicFileCopy)
                .with_guaranteed(FileSystemCapability::DurableFileCopy);
        }
        if self.config.server_side_copy {
            capabilities = capabilities.with_guaranteed(FileSystemCapability::ServerSideCopy);
        }
        if self.config.range_read {
            capabilities = capabilities.with_guaranteed(FileSystemCapability::RangeRead);
        }
        if self.config.rename_atomicity.is_some() || self.config.rename_error {
            capabilities = capabilities
                .with_guaranteed(FileSystemCapability::Rename)
                .with_guaranteed(FileSystemCapability::AtomicRename);
        }
        if self.config.atomic_temp_persist {
            capabilities = capabilities.with_guaranteed(FileSystemCapability::AtomicTempPersist);
        }
        let mut operations = ProviderOperations::new()
            .with(ProviderOperation::Stat)
            .with(ProviderOperation::List)
            .with(ProviderOperation::OpenReader)
            .with(ProviderOperation::OpenWriter)
            .with(ProviderOperation::CreateDirectory)
            .with(ProviderOperation::DeleteFile)
            .with(ProviderOperation::DeleteDirectory)
            .with(ProviderOperation::Rename)
            .with(ProviderOperation::CreateTempFile)
            .with(ProviderOperation::CreateTempDirectory);
        if self.config.omitted_capability != Some(FileSystemCapability::Copy) {
            operations = operations.with(ProviderOperation::TryCopy);
        }
        ProviderProperties::new(
            FileSystemInfo::new(
                FileSystemId::new("async-recording").expect("test id should be valid"),
                "async-recording",
                PathSemantics::Hierarchical,
            ),
            operations,
            capabilities,
            FileSystemLimits::unknown()
                .with_max_read_range_bytes(
                    self.config
                        .maximum_read_range_bytes
                        .map(FileSystemLimit::Maximum)
                        .unwrap_or(FileSystemLimit::Unknown),
                )
                .with_max_write_bytes(
                    self.config
                        .maximum_write_bytes
                        .map(FileSystemLimit::Maximum)
                        .unwrap_or(FileSystemLimit::Unknown),
                ),
            PathConstraints::absolute(),
            SymlinkPolicy::Reject,
        )
        .expect("test properties should be valid")
    }
    /// Returns an opened identity for one requested path.
    fn info(&self, path: &Path) -> OpenedFileInfo {
        let id = if self.config.invalid_opened_identity {
            FileSystemId::new("other-provider").expect("test id should be valid")
        } else {
            FileSystemId::new("async-recording").expect("test id should be valid")
        };
        OpenedFileInfo::new(id, path.clone()).with_metadata(FileMetadata::new(FileKind::File).with_len(Some(5)))
    }
    /// Returns a temporary identity, optionally invalid for boundary testing.
    fn temp_info(&self, kind: FileKind) -> OpenedFileInfo {
        let id = if self.config.invalid_temp_identity {
            FileSystemId::new("other-provider").expect("test id should be valid")
        } else {
            FileSystemId::new("async-recording").expect("test id should be valid")
        };
        let info = OpenedFileInfo::new(
            id,
            Path::parse(if self.config.invalid_temp_path {
                "relative-temp"
            } else {
                "/tmp/recording"
            })
            .expect("test path should parse"),
        );
        info.with_metadata(FileMetadata::new(kind))
    }
}
impl AsyncFileSystemSpi for AsyncRecordingSpi {
    fn properties(&self) -> ProviderProperties {
        self.properties_for()
    }
    fn stat<'a>(&'a self, request: StatRequest<'a>) -> SpiFuture<'a, FsResult<StatResponse>> {
        self.record("stat");
        if self.config.pending_stage == Some(AsyncCopyStage::Stat) {
            return Box::pin(std::future::pending());
        }
        if self.config.failing_stage == Some(AsyncCopyStage::Stat) {
            return Box::pin(async { Err(unused()) });
        }
        if request.path().as_str() == "/missing" {
            return Box::pin(async {
                Err(FsError::new(
                    FsErrorKind::NotFound,
                    FsOperation::Stat,
                    "injected missing path",
                ))
            });
        }
        let mut metadata = FileMetadata::new(self.config.stat_kind.clone().unwrap_or(FileKind::File));
        metadata = metadata.with_len(Some(5));
        let path = if self.config.invalid_stat_path {
            Path::parse("/different").expect("test path should parse")
        } else {
            request.path().clone()
        };
        Box::pin(async move { Ok(StatResponse::new(path, metadata)) })
    }
    fn list<'a>(&'a self, request: ListRequest<'a>) -> SpiFuture<'a, FsResult<OpenedAsyncDirectoryStream>> {
        let _ = request.scope().path().expect("path-scoped test request");
        let _ = request.options();
        if self.config.list_open_error {
            return Box::pin(async { Err(unused()) });
        }
        let entries = self.config.directory_entries.clone();
        let fail = self.config.directory_error;
        Box::pin(async move {
            Ok(OpenedAsyncDirectoryStream::new(Box::new(RecordingDirectorySession {
                entries,
                fail,
            })))
        })
    }
    fn open_reader<'a>(&'a self, request: OpenReaderRequest<'a>) -> SpiFuture<'a, FsResult<OpenedAsyncReader>> {
        self.record("open_reader");
        let _ = request.options();
        if self.config.pending_stage == Some(AsyncCopyStage::OpenReader) {
            return Box::pin(std::future::pending());
        }
        if self.config.failing_stage == Some(AsyncCopyStage::OpenReader) {
            return Box::pin(async { Err(unused()) });
        }
        let info = self.info(request.path());
        let config = self.config.clone();
        Box::pin(async move {
            let opened = OpenedAsyncReader::new(info, Box::new(RecordingInput { position: 0, config }));
            let _ = opened.info();
            Ok(opened)
        })
    }
    fn open_writer<'a>(&'a self, request: OpenWriterRequest<'a>) -> SpiFuture<'a, FsResult<OpenedAsyncWriter>> {
        self.record("open_writer");
        let _ = request.options();
        self.writer_options
            .lock()
            .expect("writer options lock should succeed")
            .push(request.options().options().clone());
        if self.config.pending_stage == Some(AsyncCopyStage::OpenWriter) {
            return Box::pin(std::future::pending());
        }
        if self.config.failing_stage == Some(AsyncCopyStage::OpenWriter) {
            return Box::pin(async { Err(unused().with_effect_state(FsEffectState::Unchanged)) });
        }
        if let Some(kind) = self.config.writer_open_error {
            return Box::pin(async move {
                let error = FsError::new(kind, FsOperation::OpenWriter, "injected writer-open failure");
                Err(match self.config.writer_open_effect {
                    Some(effect) => error.with_effect_state(effect),
                    None => error,
                })
            });
        }
        let info = self.info(request.path());
        let config = self.config.clone();
        Box::pin(async move {
            let opened = OpenedAsyncWriter::new(
                info,
                Box::new(RecordingWriter {
                    confirmed: 0,
                    config,
                    cancellations: Arc::clone(&self.cancellations),
                }),
            );
            let _ = opened.info();
            Ok(opened)
        })
    }
    fn create_directory<'a>(
        &'a self,
        request: CreateDirectoryRequest<'a>,
    ) -> SpiFuture<'a, FsResult<CreateDirectoryOutcome>> {
        self.record("create_directory");
        let _ = request.path();
        let _ = request.options();
        if self.config.create_directory_error {
            return Box::pin(async { Err(unused()) });
        }
        let already_existed = self.config.create_directory_already_existed;
        Box::pin(async move { Ok(CreateDirectoryOutcome::new(already_existed)) })
    }
    fn delete_file<'a>(&'a self, request: DeleteFileRequest<'a>) -> SpiFuture<'a, FsResult<DeleteOutcome>> {
        self.record("delete_file");
        let _ = request.path();
        let _ = request.options();
        if self.config.delete_error {
            return Box::pin(async { Err(unused()) });
        }
        let already_missing = self.config.delete_already_missing;
        Box::pin(async move { Ok(DeleteOutcome::new(already_missing)) })
    }
    fn delete_directory<'a>(&'a self, request: DeleteDirectoryRequest<'a>) -> SpiFuture<'a, FsResult<DeleteOutcome>> {
        self.record("delete_directory");
        let _ = request.path();
        let _ = request.options();
        if self.config.delete_error {
            return Box::pin(async { Err(unused()) });
        }
        let already_missing = self.config.delete_already_missing;
        Box::pin(async move { Ok(DeleteOutcome::new(already_missing)) })
    }
    fn try_copy<'a>(&'a self, request: CopyRequest<'a>) -> SpiFuture<'a, Result<CopyAttempt, SpiCopyFailure>> {
        self.record("try_copy");
        let _ = request.source();
        let _ = request.target();
        let _ = request.options();
        if self.config.pending_stage == Some(AsyncCopyStage::TryCopy) {
            return Box::pin(std::future::pending());
        }
        if self.config.copy_failure {
            return Box::pin(async {
                Err(SpiCopyFailure::new(
                    FsError::new(FsErrorKind::Io, FsOperation::Copy, "injected copy failure"),
                    CopyFailureState::Indeterminate,
                    CopyStats::default(),
                ))
            });
        }
        if self.config.decline_copy {
            return Box::pin(async { Ok(CopyAttempt::Declined(CopyDeclineReason::NotApplicable)) });
        }
        if let Some(atomicity) = self.config.completed_copy {
            return Box::pin(async move {
                Ok(CopyAttempt::Completed(CopyOutcome::new(
                    CopyStats::default(),
                    CopyMethod::Native,
                    atomicity,
                )))
            });
        }
        Box::pin(async { Ok(CopyAttempt::Declined(CopyDeclineReason::NotApplicable)) })
    }
    fn rename<'a>(&'a self, request: RenameRequest<'a>) -> SpiFuture<'a, Result<RenameOutcome, SpiRenameFailure>> {
        self.record("rename");
        let _ = request.options();
        if self.config.rename_error {
            return Box::pin(async { Err(SpiRenameFailure::new(unused(), RenameFailureState::Indeterminate)) });
        }
        if let Some(atomicity) = self.config.rename_atomicity {
            let source = request.source().clone();
            let target = if request.target().as_str() == "/wrong-rename-target" {
                Path::parse("/reported-rename-target").expect("generated path should parse")
            } else {
                request.target().clone()
            };
            return Box::pin(async move {
                Ok(RenameOutcome::new(
                    source,
                    target,
                    atomicity,
                    if self.config.rename_copy_then_delete {
                        PublicationMethod::CopyThenDelete
                    } else {
                        PublicationMethod::AtomicRename
                    },
                ))
            });
        }
        Box::pin(async { Err(SpiRenameFailure::new(unused(), RenameFailureState::Unchanged)) })
    }
    fn create_temp_file<'a>(&'a self, request: CreateTempFileRequest) -> SpiFuture<'a, FsResult<OpenedAsyncTempFile>> {
        self.record("create_temp_file");
        let _ = request.options();
        if self.config.temp_create_error {
            return Box::pin(async { Err(unused()) });
        }
        let info = self.temp_info(if self.config.invalid_temp_kind {
            FileKind::Directory
        } else {
            FileKind::File
        });
        let calls = Arc::clone(&self.calls);
        let temp_cancellations = Arc::clone(&self.temp_cancellations);
        Box::pin(async move {
            let opened = OpenedAsyncTempFile::new(
                info,
                Box::new(RecordingTempSession {
                    calls,
                    temp_cancellations,
                    indeterminate_persist: self.config.temp_persist_indeterminate,
                    persist_failure: self.config.temp_persist_failure,
                    cleanup_failure: self.config.temp_cleanup_failure,
                    cleanup_effect: self.config.temp_cleanup_effect,
                    pending_operation: self.config.temp_pending_operation,
                    keep_failure: self.config.temp_keep_failure,
                    atomicity: self.config.temp_persist_atomicity,
                }),
            );
            let _ = opened.info();
            Ok(opened)
        })
    }
    fn create_temp_directory<'a>(
        &'a self,
        request: CreateTempDirectoryRequest,
    ) -> SpiFuture<'a, FsResult<OpenedAsyncTempDirectory>> {
        self.record("create_temp_directory");
        let _ = request.options();
        if self.config.temp_create_error {
            return Box::pin(async { Err(unused()) });
        }
        let info = self.temp_info(FileKind::Directory);
        let calls = Arc::clone(&self.calls);
        let temp_cancellations = Arc::clone(&self.temp_cancellations);
        Box::pin(async move {
            let opened = OpenedAsyncTempDirectory::new(
                info,
                Box::new(RecordingTempSession {
                    calls,
                    temp_cancellations,
                    indeterminate_persist: self.config.temp_persist_atomicity.is_none()
                        && self.config.temp_persist_indeterminate,
                    persist_failure: self.config.temp_persist_failure,
                    cleanup_failure: self.config.temp_cleanup_failure,
                    cleanup_effect: self.config.temp_cleanup_effect,
                    pending_operation: self.config.temp_pending_operation,
                    keep_failure: self.config.temp_keep_failure,
                    atomicity: self.config.temp_persist_atomicity,
                }),
            );
            let _ = opened.info();
            Ok(opened)
        })
    }
}

/// Enumerates configured entries or one configured provider failure.
struct RecordingDirectorySession {
    entries: Vec<DirEntry>,
    fail: bool,
}
impl AsyncDirectoryStreamSession for RecordingDirectorySession {
    fn next_entry_async(&mut self) -> SpiFuture<'_, FsResult<Option<DirEntry>>> {
        if self.fail {
            self.fail = false;
            return Box::pin(async { Err(unused()) });
        }
        Box::pin(async move { Ok(self.entries.pop()) })
    }
}

/// Supplies fallback source bytes and one controllable read await point.
struct RecordingInput {
    position: usize,
    config: AsyncRecordingConfig,
}
impl AsyncInput for RecordingInput {
    type Item = u8;
    unsafe fn poll_read_unchecked(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        output: &mut [u8],
        index: usize,
        count: usize,
    ) -> Poll<IoResult<usize>> {
        let this = self.get_mut();
        if this.config.pending_stage == Some(AsyncCopyStage::ReaderRead) {
            return Poll::Pending;
        }
        if this.config.failing_stage == Some(AsyncCopyStage::ReaderRead) {
            return Poll::Ready(Err(IoError::other("injected read failure")));
        }
        let bytes = b"bytes";
        let read = bytes[this.position..].len().min(count);
        output[index..index + read].copy_from_slice(&bytes[this.position..this.position + read]);
        this.position += read;
        Poll::Ready(Ok(read))
    }
}

/// Supplies fallback destination I/O and publication behavior.
struct RecordingWriter {
    confirmed: usize,
    config: AsyncRecordingConfig,
    cancellations: Arc<Mutex<usize>>,
}
impl AsyncOutput for RecordingWriter {
    type Item = u8;
    unsafe fn poll_write_unchecked(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &[u8],
        _: usize,
        count: usize,
    ) -> Poll<IoResult<usize>> {
        let this = self.get_mut();
        let config = &this.config;
        let pending = config.pending_stage == Some(AsyncCopyStage::WriterWrite);
        let failing = config.failing_stage == Some(AsyncCopyStage::WriterWrite);
        if this.confirmed >= config.writer_progress_before_stop {
            if pending {
                return Poll::Pending;
            }
            if failing {
                return Poll::Ready(Err(IoError::other("injected write failure")));
            }
        }
        let mut accepted = count.min(config.writer_chunk.unwrap_or(count));
        if pending || failing {
            accepted = accepted.min(config.writer_progress_before_stop.saturating_sub(this.confirmed));
        }
        this.confirmed += accepted;
        Poll::Ready(Ok(accepted))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<IoResult<()>> {
        let config = self.get_mut().config.clone();
        if config.pending_stage == Some(AsyncCopyStage::WriterFlush) {
            Poll::Pending
        } else if config.failing_stage == Some(AsyncCopyStage::WriterFlush) {
            Poll::Ready(Err(IoError::other("injected flush failure")))
        } else {
            Poll::Ready(Ok(()))
        }
    }
}
impl AsyncFileWriteSession for RecordingWriter {
    fn commit_async<'a>(self: Pin<&'a mut Self>) -> SpiFuture<'a, Result<WriteOutcome, WriteFailure>> {
        let config = self.get_mut().config.clone();
        if config.pending_stage == Some(AsyncCopyStage::WriterCommit) {
            return Box::pin(std::future::pending());
        }
        Box::pin(async move {
            if let Some(state) = config.writer_commit_failure {
                return Err(WriteFailure::new(
                    FsError::new(FsErrorKind::Io, FsOperation::CommitWriter, "injected commit failure"),
                    state,
                ));
            }
            if config.failing_stage == Some(AsyncCopyStage::WriterCommit) {
                return Err(WriteFailure::new(
                    FsError::new(FsErrorKind::Io, FsOperation::CommitWriter, "injected commit failure"),
                    WriteFailureState::NotPublished,
                ));
            }
            Ok(WriteOutcome::new(
                config.writer_atomicity.unwrap_or(AchievedAtomicity::NonAtomic),
                PublicationMethod::StreamCopy,
            )
            .with_durable(config.writer_durable))
        })
    }
    fn abort_async<'a>(self: Pin<&'a mut Self>) -> SpiFuture<'a, FsResult<WriteAbortOutcome>> {
        let config = self.get_mut().config.clone();
        Box::pin(async move {
            match config.writer_abort_failure {
                Some(kind) => Err(FsError::new(kind, FsOperation::AbortWriter, "injected abort failure")),
                None => Ok(match config.writer_commit_failure {
                    Some(WriteFailureState::Published) => WriteAbortOutcome::Published,
                    Some(WriteFailureState::Indeterminate) => WriteAbortOutcome::Indeterminate,
                    Some(WriteFailureState::RetryableNotPublished) | Some(WriteFailureState::NotPublished) | None => {
                        WriteAbortOutcome::NotPublished
                    }
                }),
            }
        })
    }
    fn cancel_on_drop(self: Pin<&mut Self>) {
        *self
            .get_mut()
            .cancellations
            .lock()
            .expect("cancellation lock should succeed") += 1;
    }
}

/// Records temporary lifecycle calls and completes them successfully.
struct RecordingTempSession {
    calls: Arc<Mutex<Vec<&'static str>>>,
    temp_cancellations: Arc<Mutex<usize>>,
    indeterminate_persist: bool,
    persist_failure: Option<PersistFailureState>,
    atomicity: Option<AchievedAtomicity>,
    cleanup_failure: bool,
    cleanup_effect: Option<FsEffectState>,
    pending_operation: Option<FsOperation>,
    keep_failure: bool,
}
impl RecordingTempSession {
    /// Records one temporary lifecycle call.
    fn record(&self, call: &'static str) {
        self.calls.lock().expect("calls lock should succeed").push(call);
    }
}
impl AsyncTempResourceSpi for RecordingTempSession {
    fn cancel_on_drop(self: Pin<&mut Self>) {
        *self
            .get_mut()
            .temp_cancellations
            .lock()
            .expect("temporary cancellation lock should succeed") += 1;
    }

    fn cleanup<'a>(self: Pin<&'a mut Self>) -> SpiFuture<'a, FsResult<()>> {
        self.as_ref().get_ref().record("cleanup");
        let cleanup_failure = self.as_ref().get_ref().cleanup_failure;
        Box::pin(async move {
            if self.as_ref().get_ref().pending_operation == Some(FsOperation::CleanupTemp) {
                std::future::pending::<()>().await;
            }
            if let Some(effect) = self.as_ref().get_ref().cleanup_effect {
                return Err(
                    FsError::new(FsErrorKind::Io, FsOperation::CleanupTemp, "injected cleanup effect")
                        .with_effect_state(effect),
                );
            }
            if cleanup_failure {
                Err(FsError::with_source(
                    FsErrorKind::Io,
                    FsOperation::CleanupTemp,
                    "injected cleanup failure",
                    IoError::other("underlying cleanup failure"),
                ))
            } else {
                Ok(())
            }
        })
    }
    fn keep<'a>(self: Pin<&'a mut Self>) -> SpiFuture<'a, Result<PersistOutcome, SpiPersistFailure>> {
        self.as_ref().get_ref().record("keep");
        let keep_failure = self.as_ref().get_ref().keep_failure;
        let failure_state = self.as_ref().get_ref().persist_failure;
        Box::pin(async move {
            if self.as_ref().get_ref().pending_operation == Some(FsOperation::KeepTemp) {
                std::future::pending::<()>().await;
            }
            if let Some(state) = failure_state {
                return Err(SpiPersistFailure::new(
                    FsError::new(FsErrorKind::Io, FsOperation::KeepTemp, "injected keep failure")
                        .with_target(Path::parse("/kept-resource").expect("generated target")),
                    state,
                ));
            }
            if keep_failure {
                Err(SpiPersistFailure::new(
                    FsError::new(FsErrorKind::Io, FsOperation::KeepTemp, "injected keep failure"),
                    PersistFailureState::NotPublished,
                ))
            } else {
                Ok(PersistOutcome::new(
                    Path::parse("/kept-resource").expect("generated path should parse"),
                    AchievedAtomicity::Atomic,
                    PublicationMethod::Direct,
                ))
            }
        })
    }
    fn persist<'a>(
        self: Pin<&'a mut Self>,
        request: PersistRequest<'a>,
    ) -> SpiFuture<'a, Result<PersistOutcome, SpiPersistFailure>> {
        self.as_ref().get_ref().record("persist");
        let target = if request.target().as_str() == "/wrong-persist-target" {
            Path::parse("/reported-persist-target").expect("generated path should parse")
        } else {
            request.target().clone()
        };
        let _ = request.options();
        let indeterminate = self.as_ref().get_ref().indeterminate_persist;
        let failure = self.as_ref().get_ref().persist_failure;
        Box::pin(async move {
            if self.as_ref().get_ref().pending_operation == Some(FsOperation::PersistTemp) {
                std::future::pending::<()>().await;
            }
            if let Some(state) = failure {
                return Err(SpiPersistFailure::new(
                    FsError::new(FsErrorKind::Io, FsOperation::PersistTemp, "injected persist failure"),
                    state,
                ));
            }
            if indeterminate {
                return Err(SpiPersistFailure::new(
                    FsError::new(
                        FsErrorKind::Indeterminate,
                        FsOperation::PersistTemp,
                        "injected indeterminate persist",
                    ),
                    PersistFailureState::Indeterminate,
                ));
            }
            Ok(PersistOutcome::new(
                target,
                self.as_ref().get_ref().atomicity.unwrap_or(AchievedAtomicity::Atomic),
                PublicationMethod::AtomicRename,
            ))
        })
    }
}

/// Builds an error for unsupported provider methods outside this test scope.
fn unused() -> FsError {
    FsError::new(FsErrorKind::UnsupportedOperation, FsOperation::Other, "unused")
}
