// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
// Deterministic in-memory provider used only by executable Rustdoc examples.

#[cfg(feature = "async")]
use qubit_fs::AsyncFileSystem;

pub mod rustdoc_provider {
    use std::collections::BTreeMap;
    use std::io::Cursor;
    use std::io::Result as IoResult;
    use std::sync::Arc;
    use std::sync::Mutex;

    use qubit_fs::FileSystem;
    use qubit_fs::Path;
    use qubit_fs::error::FsError;
    use qubit_fs::error::FsErrorKind;
    use qubit_fs::error::FsOperation;
    use qubit_fs::error::FsResult;
    use qubit_fs::metadata::AchievedAtomicity;
    use qubit_fs::metadata::FileKind;
    use qubit_fs::metadata::FileMetadata;
    use qubit_fs::metadata::FileSystemCapabilities;
    use qubit_fs::metadata::FileSystemCapability;
    use qubit_fs::metadata::FileSystemId;
    use qubit_fs::metadata::FileSystemInfo;
    use qubit_fs::metadata::FileSystemLimits;
    use qubit_fs::metadata::OpenedFileInfo;
    use qubit_fs::metadata::PublicationMethod;
    use qubit_fs::metadata::SymlinkPolicy;
    use qubit_fs::metadata::WriteOutcome;
    use qubit_fs::path::PathConstraints;
    use qubit_fs::path::PathSemantics;
    use qubit_fs::spi::CreateTempFileRequest;
    use qubit_fs::spi::FileSystemSpi;
    use qubit_fs::spi::FileWriterSpi;
    use qubit_fs::spi::OpenReaderRequest;
    use qubit_fs::spi::OpenWriterRequest;
    use qubit_fs::spi::OpenedReader;
    use qubit_fs::spi::OpenedTempFile;
    use qubit_fs::spi::OpenedWriter;
    use qubit_fs::spi::PersistRequest;
    use qubit_fs::spi::ProviderOperation;
    use qubit_fs::spi::ProviderOperations;
    use qubit_fs::spi::ProviderProperties;
    use qubit_fs::spi::SpiPersistFailure;
    use qubit_fs::spi::SpiWriteFailure;
    use qubit_fs::spi::StatRequest;
    use qubit_fs::spi::StatResponse;
    use qubit_fs::spi::TempResourceSpi;
    use qubit_fs::temp::PersistFailureState;
    use qubit_fs::temp::PersistOutcome;
    use qubit_fs::write::WriteAbortOutcome;
    use qubit_fs::write::WriteDisposition;
    use qubit_fs::write::WriteFailureState;
    use qubit_io::Output;

    type Files = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;

    /// Creates isolated storage containing `/report` with six bytes.
    pub fn filesystem() -> FileSystem {
        let files = BTreeMap::from([("/report".to_owned(), b"report".to_vec())]);
        FileSystem::from_spi(Memory {
            files: Arc::new(Mutex::new(files)),
        })
        .unwrap()
    }

    struct Memory {
        files: Files,
    }
    impl Memory {
        fn info(&self, path: Path) -> OpenedFileInfo {
            OpenedFileInfo::new(FileSystemId::new("rustdoc").unwrap(), path)
                .with_metadata(FileMetadata::new(FileKind::File))
        }
    }
    impl FileSystemSpi for Memory {
        fn properties(&self) -> ProviderProperties {
            ProviderProperties::new(
                FileSystemInfo::new(
                    FileSystemId::new("rustdoc").unwrap(),
                    "rustdoc",
                    PathSemantics::Hierarchical,
                ),
                ProviderOperations::new()
                    .with(ProviderOperation::Stat)
                    .with(ProviderOperation::OpenReader)
                    .with(ProviderOperation::OpenWriter)
                    .with(ProviderOperation::CreateTempFile),
                FileSystemCapabilities::new()
                    .with_guaranteed(FileSystemCapability::Read)
                    .with_guaranteed(FileSystemCapability::Write)
                    .with_guaranteed(FileSystemCapability::TempFile),
                FileSystemLimits::unknown(),
                PathConstraints::absolute(),
                SymlinkPolicy::Reject,
            )
            .unwrap()
        }
        fn stat(&self, request: StatRequest<'_>) -> FsResult<StatResponse> {
            let files = self.files.lock().unwrap();
            let bytes = files
                .get(request.path().as_str())
                .ok_or_else(|| missing(FsOperation::Stat))?;
            Ok(StatResponse::new(
                request.path().clone(),
                FileMetadata::new(FileKind::File).with_len(Some(bytes.len() as u64)),
            ))
        }
        fn open_reader(&self, request: OpenReaderRequest<'_>) -> FsResult<OpenedReader> {
            let files = self.files.lock().unwrap();
            let bytes = files
                .get(request.path().as_str())
                .ok_or_else(|| missing(FsOperation::OpenReader))?
                .clone();
            Ok(OpenedReader::new(
                self.info(request.path().clone()),
                Box::new(Cursor::new(bytes)),
            ))
        }
        fn open_writer(&self, request: OpenWriterRequest<'_>) -> FsResult<OpenedWriter> {
            Ok(OpenedWriter::new(
                self.info(request.path().clone()),
                Box::new(Writer {
                    files: Arc::clone(&self.files),
                    path: request.path().as_str().to_owned(),
                    bytes: Vec::new(),
                    create_new: request.options().options().disposition() == WriteDisposition::CreateNew,
                }),
            ))
        }
        fn create_temp_file(&self, _: CreateTempFileRequest) -> FsResult<OpenedTempFile> {
            let mut files = self.files.lock().unwrap();
            let path = Path::parse(&format!("/scratch-{}", files.len())).unwrap();
            files.insert(path.as_str().to_owned(), Vec::new());
            Ok(OpenedTempFile::new(
                self.info(path.clone()),
                Box::new(Temporary {
                    files: Arc::clone(&self.files),
                    source: path,
                }),
            ))
        }
    }
    fn missing(operation: FsOperation) -> FsError {
        FsError::new(FsErrorKind::NotFound, operation, "example resource missing")
    }
    struct Writer {
        files: Files,
        path: String,
        bytes: Vec<u8>,
        create_new: bool,
    }
    impl Output for Writer {
        type Item = u8;
        unsafe fn write_unchecked(&mut self, bytes: &[u8], index: usize, count: usize) -> IoResult<usize> {
            self.bytes.extend_from_slice(&bytes[index..index + count]);
            Ok(count)
        }
        fn flush(&mut self) -> IoResult<()> {
            Ok(())
        }
    }
    impl FileWriterSpi for Writer {
        fn commit(&mut self) -> Result<WriteOutcome, SpiWriteFailure> {
            let mut files = self.files.lock().unwrap();
            if self.create_new && files.contains_key(&self.path) {
                return Err(SpiWriteFailure::new(
                    FsError::new(
                        FsErrorKind::AlreadyExists,
                        FsOperation::CommitWriter,
                        "example target exists",
                    ),
                    WriteFailureState::NotPublished,
                ));
            }
            files.insert(self.path.clone(), self.bytes.clone());
            Ok(
                WriteOutcome::new(AchievedAtomicity::NonAtomic, PublicationMethod::Direct)
                    .with_bytes_written(self.bytes.len() as u64),
            )
        }
        fn abort(&mut self) -> FsResult<WriteAbortOutcome> {
            Ok(WriteAbortOutcome::NotPublished)
        }
    }
    struct Temporary {
        files: Files,
        source: Path,
    }
    impl Temporary {
        fn publish(&mut self, target: Path) -> Result<PersistOutcome, SpiPersistFailure> {
            let mut files = self.files.lock().unwrap();
            let bytes = files.remove(self.source.as_str()).ok_or_else(|| {
                SpiPersistFailure::new(
                    missing(FsOperation::PersistTemp),
                    PersistFailureState::NotPublishedSourceReleased,
                )
            })?;
            files.insert(target.as_str().to_owned(), bytes);
            Ok(PersistOutcome::new(
                target,
                AchievedAtomicity::NonAtomic,
                PublicationMethod::CopyThenDelete,
            ))
        }
    }
    impl TempResourceSpi for Temporary {
        fn persist(&mut self, request: PersistRequest<'_>) -> Result<PersistOutcome, SpiPersistFailure> {
            self.publish(request.target().clone())
        }
        fn keep(&mut self) -> Result<PersistOutcome, SpiPersistFailure> {
            self.publish(Path::parse("/kept").unwrap())
        }
        fn cleanup(&mut self) -> FsResult<()> {
            self.files.lock().unwrap().remove(self.source.as_str());
            Ok(())
        }
    }
}

#[cfg(feature = "async")]
#[path = "async_recording_spi.rs"]
pub mod async_recording_spi;
#[path = "poll_support.rs"]
pub mod poll_support;

/// Creates the shared asynchronous facade used by Rustdoc examples.
#[cfg(feature = "async")]
pub fn async_filesystem() -> AsyncFileSystem {
    async_recording_spi::async_recording_file_system(async_recording_spi::AsyncRecordingConfig::default()).0
}
