use crate::error_mapper;
use bytes::Bytes;
use object_store::{ObjectStore, PutMode, PutOptions, path::Path as ObjectPath};
use qubit_fs::error::{FsErrorKind, FsOperation};
use qubit_fs::metadata::{AchievedAtomicity, PublicationMethod, WriteOutcome};
use qubit_fs::spi::AsyncFileWriteSession;
use qubit_fs::write::{WriteAbortOutcome, WriteFailure};
use qubit_fs::{FsError, FsResult};
use qubit_io::AsyncOutput;
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

pub struct S3WriteSession {
    store: Arc<dyn ObjectStore>,
    key: ObjectPath,
    data: Vec<u8>,
    state: State,
}
enum State {
    Open,
    Published,
    Indeterminate,
}
impl S3WriteSession {
    pub fn open(
        store: Arc<dyn ObjectStore>,
        key: String,
        options: &qubit_fs::write::WriteOptions,
    ) -> Result<Self, FsError> {
        if options.disposition() != qubit_fs::write::WriteDisposition::CreateNew
            || options.content_type().is_some()
            || options.checksum().is_some()
        {
            return Err(FsError::new(
                FsErrorKind::RequirementNotMet,
                FsOperation::OpenWriter,
                "S3 contract writer only supports plain CreateNew",
            ));
        }
        Ok(Self {
            store,
            key: ObjectPath::parse(key).map_err(|e| {
                FsError::with_source(
                    FsErrorKind::InvalidPath,
                    FsOperation::OpenWriter,
                    "invalid S3 object key",
                    e,
                )
            })?,
            data: Vec::new(),
            state: State::Open,
        })
    }
}
impl AsyncOutput for S3WriteSession {
    type Item = u8;
    unsafe fn poll_write_unchecked(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        input: &[u8],
        index: usize,
        count: usize,
    ) -> Poll<std::io::Result<usize>> {
        if self.data.len().saturating_add(count) > 1_048_576 {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "S3 contract writer limit exceeded",
            )));
        }
        self.data.extend_from_slice(&input[index..index + count]);
        Poll::Ready(Ok(count))
    }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
impl AsyncFileWriteSession for S3WriteSession {
    fn commit_async<'a>(
        self: Pin<&'a mut Self>,
    ) -> qubit_fs::spi::SpiFuture<'a, Result<WriteOutcome, WriteFailure>> {
        Box::pin(async move {
            if !matches!(self.state, State::Open) {
                return Err(WriteFailure::new(
                    FsError::new(
                        FsErrorKind::InvalidState,
                        FsOperation::CommitWriter,
                        "writer already committed",
                    ),
                    qubit_fs::write::WriteFailureState::Published,
                ));
            }
            let this = self.get_mut();
            let count = this.data.len() as u64;
            let result = this
                .store
                .put_opts(
                    &this.key,
                    Bytes::from(std::mem::take(&mut this.data)).into(),
                    PutOptions {
                        mode: PutMode::Create,
                        ..Default::default()
                    },
                )
                .await;
            match result {
                Ok(_) => {
                    this.state = State::Published;
                    Ok(
                        WriteOutcome::new(AchievedAtomicity::Atomic, PublicationMethod::Direct)
                            .with_bytes_written(count),
                    )
                }
                Err(e) => {
                    this.state = State::Indeterminate;
                    Err(WriteFailure::new(
                        error_mapper::map(e, FsOperation::CommitWriter),
                        qubit_fs::write::WriteFailureState::Indeterminate,
                    ))
                }
            }
        })
    }
    fn abort_async<'a>(
        self: Pin<&'a mut Self>,
    ) -> qubit_fs::spi::SpiFuture<'a, FsResult<WriteAbortOutcome>> {
        Box::pin(async move {
            let this = self.get_mut();
            Ok(match this.state {
                State::Open => {
                    this.data.clear();
                    WriteAbortOutcome::NotPublished
                }
                State::Published => WriteAbortOutcome::Published,
                State::Indeterminate => WriteAbortOutcome::Indeterminate,
            })
        })
    }
}
