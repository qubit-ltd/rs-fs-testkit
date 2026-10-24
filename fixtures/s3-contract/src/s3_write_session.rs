// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::io::Error as IoError;
use std::io::ErrorKind as IoErrorKind;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;

use bytes::Bytes;
use object_store::Error as ObjectStoreError;
use object_store::ObjectStore;
use object_store::PutMode;
use object_store::PutOptions;
use object_store::UpdateVersion;
use object_store::path::Path as ObjectPath;
use qubit_fs::FsError;
use qubit_fs::FsResult;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::AchievedAtomicity;
use qubit_fs::metadata::AtomicityRequirement;
use qubit_fs::metadata::DurabilityRequirement;
use qubit_fs::metadata::PublicationMethod;
use qubit_fs::metadata::WriteOutcome;
use qubit_fs::spi::AsyncFileWriteSession;
use qubit_fs::spi::SpiFuture;
use qubit_fs::write::WriteAbortOutcome;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteFailure;
use qubit_fs::write::WriteFailureState;
use qubit_fs::write::WriteOptions;
use qubit_fs::write::WritePrecondition;
use qubit_io::AsyncOutput;

use self::internal::State;
use crate::TestControl;
use crate::TestStage;
use crate::error_mapper;

mod internal;

/// Buffers a bounded S3 object write until its single PUT is committed.
pub struct S3WriteSession {
    /// Shared object store used to publish the buffered bytes.
    store: Arc<dyn ObjectStore>,
    /// Destination object key.
    key: ObjectPath,
    /// Bytes accepted by the writer before commit.
    data: Vec<u8>,
    /// Publication state used to report commit and abort outcomes.
    state: State,
    /// Conditional publication mode passed to the object store.
    mode: PutMode,
    /// Whether a failed conditional create should be reported as a precondition failure.
    if_absent: bool,
    /// Fixture controls for pausing and observing write lifecycle stages.
    control: TestControl,
}

impl S3WriteSession {
    /// Creates a writer for supported single-PUT options.
    ///
    /// # Parameters
    ///
    /// - `store`: object store that receives the completed PUT.
    /// - `key`: destination object key to parse and write.
    /// - `options`: write requirements, which must be supported by a single PUT.
    /// - `control`: fixture controls for observing and pausing lifecycle stages.
    ///
    /// # Returns
    ///
    /// A session that buffers bytes until commit.
    ///
    /// # Errors
    ///
    /// Returns an error if the options require unsupported behavior or if `key` is invalid.
    pub fn open(
        store: Arc<dyn ObjectStore>,
        key: String,
        options: &WriteOptions,
        control: TestControl,
    ) -> Result<Self, FsError> {
        let mode = match (options.disposition(), options.precondition()) {
            (
                WriteDisposition::CreateNew,
                WritePrecondition::None | WritePrecondition::IfAbsent,
            )
            | (WriteDisposition::CreateOrReplace, WritePrecondition::IfAbsent) => PutMode::Create,
            (WriteDisposition::CreateOrReplace, WritePrecondition::IfMatch(version)) => {
                PutMode::Update(UpdateVersion {
                    e_tag: Some(version.as_ref().to_owned()),
                    version: None,
                })
            }
            _ => return Err(unsupported_write_options()),
        };
        if options.create_parent()
            || options.atomicity() == AtomicityRequirement::Required
            || options.durability() != DurabilityRequirement::NotRequired
            || options.content_type().is_some()
            || options.checksum().is_some()
            || !options.user_metadata().is_empty()
        {
            return Err(unsupported_write_options());
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
            mode,
            if_absent: options.precondition() == &WritePrecondition::IfAbsent,
            control,
        })
    }
}
impl AsyncOutput for S3WriteSession {
    type Item = u8;

    /// Copies the requested input range into the session's bounded buffer.
    ///
    /// The bytes are not published until `commit_async` succeeds. The write
    /// stage may remain pending while the fixture control gate is closed, and
    /// the buffer rejects writes that would exceed its 1 MiB limit.
    ///
    /// # Parameters
    ///
    /// - `cx`: Task context used to register interest while the write gate is
    ///   closed.
    /// - `input`: Source bytes containing the requested range.
    /// - `index`: Start of the requested range in `input`.
    /// - `count`: Number of bytes requested from that range.
    ///
    /// # Returns
    ///
    /// Returns `Pending` while the write gate is closed, or the accepted byte
    /// count once the gate opens.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the session no longer accepts bytes or the
    /// write would exceed the buffer limit.
    ///
    /// # Safety
    ///
    /// `index..index + count` must be a valid range within `input`, and the
    /// addition must not overflow.
    unsafe fn poll_write_unchecked(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        input: &[u8],
        index: usize,
        count: usize,
    ) -> Poll<std::io::Result<usize>> {
        if self.control.poll_gate(TestStage::Write, cx).is_pending() {
            return Poll::Pending;
        }
        if !matches!(self.state, State::Open) {
            return Poll::Ready(Err(IoError::other("writer no longer accepts bytes")));
        }
        if self.data.len().saturating_add(count) > 1_048_576 {
            return Poll::Ready(Err(IoError::new(
                IoErrorKind::WriteZero,
                "S3 contract writer limit exceeded",
            )));
        }
        self.data.extend_from_slice(&input[index..index + count]);
        self.control.record_write(count);
        Poll::Ready(Ok(count))
    }
    /// Polls the fixture's flush gate without publishing buffered bytes.
    ///
    /// A pending result registers the current task with the gate; after the
    /// gate opens, flushing completes immediately because publication is
    /// performed by `commit_async`.
    ///
    /// # Parameters
    ///
    /// - `cx`: Task context used to register interest while the flush gate is
    ///   closed.
    ///
    /// # Returns
    ///
    /// Returns `Pending` while the gate is closed, or a successful flush result
    /// after it opens.
    ///
    /// # Errors
    ///
    /// The fixture gate does not produce a flush error.
    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.control
            .poll_gate(TestStage::Flush, cx)
            .map(|()| Ok(()))
    }
}
impl AsyncFileWriteSession for S3WriteSession {
    /// Publishes the complete buffered object with one conditional S3 PUT.
    ///
    /// The returned outcome reports the bytes accepted before publication.
    /// Errors preserve whether the PUT is known not to have published or its
    /// result is indeterminate; the future may pause at fixture-controlled
    /// lifecycle stages before and after the PUT.
    ///
    /// # Returns
    ///
    /// Resolves to the publication outcome and accepted byte count.
    ///
    /// # Errors
    ///
    /// Resolves to a `WriteFailure` that records the filesystem error and
    /// whether publication was prevented or remains indeterminate.
    fn commit_async<'a>(
        self: Pin<&'a mut Self>,
    ) -> SpiFuture<'a, Result<WriteOutcome, WriteFailure>> {
        Box::pin(async move {
            if !matches!(self.state, State::Open) {
                return Err(WriteFailure::new(
                    FsError::new(
                        FsErrorKind::InvalidState,
                        FsOperation::CommitWriter,
                        "writer already committed",
                    ),
                    match self.state {
                        State::Published => WriteFailureState::Published,
                        _ => WriteFailureState::Indeterminate,
                    },
                ));
            }
            let this = self.get_mut();
            let count = this.data.len() as u64;
            let payload = Bytes::copy_from_slice(&this.data);
            this.control.wait(TestStage::BeforePut).await;
            // Once PUT can be polled, cancellation cannot prove non-publication.
            this.state = State::Indeterminate;
            this.control.record_put();
            let result = this
                .store
                .put_opts(
                    &this.key,
                    payload.into(),
                    PutOptions {
                        mode: this.mode.clone(),
                        ..Default::default()
                    },
                )
                .await;
            match result {
                Ok(_) => {
                    this.data.clear();
                    this.state = State::Published;
                    this.control.wait(TestStage::AfterPutBeforeResult).await;
                    Ok(
                        WriteOutcome::new(AchievedAtomicity::Atomic, PublicationMethod::Direct)
                            .with_bytes_written(count),
                    )
                }
                Err(e) => {
                    this.state = if matches!(
                        e,
                        ObjectStoreError::AlreadyExists { .. }
                            | ObjectStoreError::Precondition { .. }
                    ) {
                        State::Open
                    } else {
                        State::Indeterminate
                    };
                    let error =
                        if this.if_absent && matches!(e, ObjectStoreError::AlreadyExists { .. }) {
                            FsError::with_source(
                                FsErrorKind::PreconditionFailed,
                                FsOperation::CommitWriter,
                                "S3 conditional creation found an existing object",
                                e,
                            )
                        } else {
                            error_mapper::map(e, FsOperation::CommitWriter)
                        };
                    Err(WriteFailure::new(
                        error,
                        if matches!(this.state, State::Open) {
                            WriteFailureState::NotPublished
                        } else {
                            WriteFailureState::Indeterminate
                        },
                    ))
                }
            }
        })
    }
    /// Discards buffered bytes when unpublished, or reports the known remote
    /// publication state without attempting another PUT.
    ///
    /// Cleanup can pause at the fixture's abort gate and can return an
    /// injected I/O error. Published and indeterminate sessions retain those
    /// outcomes because local cancellation cannot undo or resolve the PUT.
    ///
    /// # Returns
    ///
    /// Resolves to whether the object was not published, published, or has an
    /// indeterminate publication state.
    ///
    /// # Errors
    ///
    /// Resolves to a filesystem error if fixture cleanup is configured to
    /// fail.
    fn abort_async<'a>(self: Pin<&'a mut Self>) -> SpiFuture<'a, FsResult<WriteAbortOutcome>> {
        Box::pin(async move {
            let this = self.get_mut();
            let fail = this.control.begin_abort();
            this.control.wait(TestStage::Abort).await;
            if fail {
                return Err(FsError::new(
                    FsErrorKind::Io,
                    FsOperation::AbortWriter,
                    "injected abort failure",
                ));
            }
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

/// Constructs the error returned for options outside the fixture's supported write contract.
///
/// The S3 fixture accepts create-new writes or ETag-conditional replacements
/// using best-effort, untyped write requirements.
fn unsupported_write_options() -> FsError {
    FsError::new(
        FsErrorKind::RequirementNotMet,
        FsOperation::OpenWriter,
        "S3 fixture supports create-new or ETag-conditional replacement with best-effort untyped writes",
    )
}
