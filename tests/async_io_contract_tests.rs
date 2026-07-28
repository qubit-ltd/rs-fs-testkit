// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::{
    collections::HashMap,
    future::Future,
    io,
    pin::Pin,
    sync::{
        Arc,
        Mutex,
    },
    task::{
        Context,
        Poll,
        Waker,
    },
};

use qubit_fs::{
    AchievedAtomicity,
    AsyncFileReader,
    AsyncFileSystem,
    AsyncFileWriteSession,
    AsyncFileWriter,
    FileKind,
    FileLocation,
    FileMetadata,
    FileSystemCapabilities,
    FileSystemCapability,
    FileSystemId,
    FileSystemInfo,
    FileSystemLimits,
    FileSystemProperties,
    FsError,
    FsErrorKind,
    FsFuture,
    FsOperation,
    FsPath,
    OpenedFileInfo,
    PathSemantics,
    PublicationMethod,
    ReadOptions,
    WriteFuture,
    WriteOptions,
    WriteOutcome,
};
use qubit_fs_testkit::{
    AsyncFileSystemFixture,
    assert_async_write_contract,
};
use qubit_io::{
    AsyncInput,
    AsyncOutput,
};

type Entries = Arc<Mutex<HashMap<String, Vec<u8>>>>;

struct MemoryAsyncFixture {
    file_system: MemoryAsyncFileSystem,
}

impl MemoryAsyncFixture {
    fn new() -> Self {
        Self {
            file_system: MemoryAsyncFileSystem::new(),
        }
    }
}

impl AsyncFileSystemFixture for MemoryAsyncFixture {
    fn file_system(&self) -> &dyn AsyncFileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> FsPath {
        FsPath::parse(&format!("/{relative}"))
            .expect("fixture path should parse")
    }
}

struct MemoryAsyncFileSystem {
    info: FileSystemInfo,
    limits: FileSystemLimits,
    entries: Entries,
}

impl MemoryAsyncFileSystem {
    fn new() -> Self {
        Self {
            info: FileSystemInfo::new(
                FileSystemId::new("async-memory")
                    .expect("fixture filesystem ID should be valid"),
                "async-memory",
                PathSemantics::Hierarchical,
            ),
            limits: FileSystemLimits::unknown(),
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn opened_info(&self, path: &FsPath) -> OpenedFileInfo {
        OpenedFileInfo::new(FileLocation::new(
            self.info.id().clone(),
            path.clone(),
        ))
    }
}

impl FileSystemProperties for MemoryAsyncFileSystem {
    fn info(&self) -> &FileSystemInfo {
        &self.info
    }

    fn capabilities(&self) -> FileSystemCapabilities {
        FileSystemCapabilities::default()
            .with(FileSystemCapability::Read)
            .with(FileSystemCapability::Write)
    }

    fn limits(&self) -> &FileSystemLimits {
        &self.limits
    }
}

impl AsyncFileSystem for MemoryAsyncFileSystem {
    fn stat_async<'a>(
        &'a self,
        path: &'a FsPath,
    ) -> FsFuture<'a, FileMetadata> {
        let result = self
            .entries
            .lock()
            .expect("fixture entries lock should succeed")
            .get(path.as_str())
            .map(|content| {
                let mut metadata = FileMetadata::new(FileKind::File);
                metadata.len = Some(content.len() as u64);
                metadata
            })
            .ok_or_else(|| {
                FsError::new(
                    FsErrorKind::NotFound,
                    FsOperation::Stat,
                    "fixture entry does not exist",
                )
                .with_path(path.clone())
            });
        Box::pin(async move { result })
    }

    fn open_reader_async<'a>(
        &'a self,
        path: &'a FsPath,
        _options: ReadOptions,
    ) -> FsFuture<'a, AsyncFileReader> {
        let result = self
            .entries
            .lock()
            .expect("fixture entries lock should succeed")
            .get(path.as_str())
            .cloned()
            .map(|content| {
                AsyncFileReader::new(
                    MemoryAsyncInput {
                        content,
                        position: 0,
                    },
                    self.opened_info(path),
                )
            })
            .ok_or_else(|| {
                FsError::new(
                    FsErrorKind::NotFound,
                    FsOperation::OpenReader,
                    "fixture entry does not exist",
                )
                .with_path(path.clone())
            });
        Box::pin(async move { result })
    }

    fn open_writer_async<'a>(
        &'a self,
        path: &'a FsPath,
        _options: WriteOptions,
    ) -> FsFuture<'a, AsyncFileWriter> {
        let writer = AsyncFileWriter::new(
            MemoryAsyncWriteSession {
                entries: self.entries.clone(),
                path: path.as_str().to_owned(),
                pending: Vec::new(),
            },
            self.opened_info(path),
        );
        Box::pin(async move { Ok(writer) })
    }
}

struct MemoryAsyncInput {
    content: Vec<u8>,
    position: usize,
}

impl AsyncInput for MemoryAsyncInput {
    type Item = u8;

    unsafe fn poll_read_unchecked(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        output: &mut [u8],
        index: usize,
        count: usize,
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let available = this.content.len().saturating_sub(this.position);
        let read = available.min(count);
        output[index..index + read].copy_from_slice(
            &this.content[this.position..this.position + read],
        );
        this.position += read;
        Poll::Ready(Ok(read))
    }
}

struct MemoryAsyncWriteSession {
    entries: Entries,
    path: String,
    pending: Vec<u8>,
}

impl AsyncOutput for MemoryAsyncWriteSession {
    type Item = u8;

    unsafe fn poll_write_unchecked(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        input: &[u8],
        index: usize,
        count: usize,
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.pending.extend_from_slice(&input[index..index + count]);
        Poll::Ready(Ok(count))
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl AsyncFileWriteSession for MemoryAsyncWriteSession {
    fn commit_async<'a>(self: Pin<&'a mut Self>) -> WriteFuture<'a> {
        let this = self.get_mut();
        this.entries
            .lock()
            .expect("fixture entries lock should succeed")
            .insert(this.path.clone(), this.pending.clone());
        Box::pin(async {
            Ok(WriteOutcome::new(
                AchievedAtomicity::NonAtomic,
                PublicationMethod::Direct,
            ))
        })
    }

    fn abort_async<'a>(self: Pin<&'a mut Self>) -> FsFuture<'a, ()> {
        self.get_mut().pending.clear();
        Box::pin(async { Ok(()) })
    }
}

fn ready<F>(future: F) -> F::Output
where
    F: Future,
{
    let mut context = Context::from_waker(Waker::noop());
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("fixture future should be immediately ready"),
    }
}

#[test]
fn test_async_write_contract_accepts_conforming_provider() {
    ready(assert_async_write_contract(&MemoryAsyncFixture::new()))
        .expect("conforming asynchronous provider should satisfy the contract");
}
