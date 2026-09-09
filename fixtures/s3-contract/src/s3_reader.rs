use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;

use bytes::Bytes;
use futures_util::Stream;
use object_store::GetOptions;
use object_store::ObjectStore;
use object_store::path::Path as ObjectPath;
use qubit_fs::FsError;
use qubit_fs::Path;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileKind;
use qubit_fs::metadata::FileMetadata;
use qubit_fs::metadata::FileSystemId;
use qubit_fs::metadata::OpenedFileInfo;
use qubit_fs::read::ReadOptions;
use qubit_io::AsyncInput;

use crate::error_mapper;

type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, object_store::Error>> + Send>>;
pub struct S3Reader {
    info: OpenedFileInfo,
    stream: ByteStream,
    chunk: Bytes,
    offset: usize,
}

impl S3Reader {
    pub async fn open(
        store: Arc<dyn ObjectStore>,
        key: String,
        path: Path,
        options: &ReadOptions,
    ) -> Result<Self, FsError> {
        let mut get = GetOptions {
            if_match: options.if_match().map(|v| v.as_ref().to_owned()),
            if_none_match: options.if_none_match().map(|v| v.as_ref().to_owned()),
            ..Default::default()
        };
        let offset = options.offset().unwrap_or(0);
        if let Some(length) = options.length() {
            let end = offset
                .checked_add(length)
                .ok_or_else(|| FsError::invalid_path(FsOperation::OpenReader, "read range overflows"))?;
            get.range = Some((offset..end).into());
        } else if offset != 0 {
            get.range = Some((offset..).into());
        }
        let result = store
            .get_opts(
                &ObjectPath::parse(key).map_err(|e| {
                    FsError::with_source(
                        FsErrorKind::InvalidPath,
                        FsOperation::OpenReader,
                        "invalid S3 object key",
                        e,
                    )
                })?,
                get,
            )
            .await
            .map_err(|e| error_mapper::map(e, FsOperation::OpenReader))?;
        let metadata = FileMetadata::new(FileKind::Object)
            .with_len(Some(result.meta.size as u64))
            .with_etag(result.meta.e_tag.clone().map(Into::into));
        Ok(Self {
            info: OpenedFileInfo::new(FileSystemId::new("s3-contract").unwrap(), path).with_metadata(metadata),
            stream: Box::pin(result.into_stream()),
            chunk: Bytes::new(),
            offset: 0,
        })
    }
    pub fn info(&self) -> &OpenedFileInfo {
        &self.info
    }
}

impl AsyncInput for S3Reader {
    type Item = u8;
    unsafe fn poll_read_unchecked(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        output: &mut [u8],
        index: usize,
        count: usize,
    ) -> Poll<std::io::Result<usize>> {
        if count == 0 {
            return Poll::Ready(Ok(0));
        }
        loop {
            if self.offset < self.chunk.len() {
                let n = count.min(self.chunk.len() - self.offset);
                output[index..index + n].copy_from_slice(&self.chunk[self.offset..self.offset + n]);
                self.offset += n;
                return Poll::Ready(Ok(n));
            }
            match self.stream.as_mut().poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(Ok(0)),
                Poll::Ready(Some(Ok(bytes))) => {
                    self.chunk = bytes;
                    self.offset = 0;
                }
                Poll::Ready(Some(Err(error))) => {
                    return Poll::Ready(Err(error_mapper::map(error, FsOperation::Read).into_io_error()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    /// Stream failures retain their filesystem category and original SDK
    /// source.
    #[tokio::test]
    async fn stream_error_retains_typed_source() {
        let error = object_store::Error::Precondition {
            path: "key".into(),
            source: Box::new(std::io::Error::other("SDK source")),
        };
        let mut reader = S3Reader {
            info: OpenedFileInfo::new(
                FileSystemId::new("s3-contract").unwrap(),
                Path::parse_literal("key").unwrap(),
            ),
            stream: Box::pin(futures_util::stream::iter([Err(error)])),
            chunk: Bytes::new(),
            offset: 0,
        };
        let mut output = [0; 1];
        let error = reader.read_async(&mut output).await.unwrap_err();
        let error = error.get_ref().unwrap().downcast_ref::<FsError>().unwrap();
        assert_eq!(error.kind(), FsErrorKind::PreconditionFailed);
        let mut cause: &dyn Error = error;
        let mut found_sdk = false;
        while let Some(source) = cause.source() {
            found_sdk |= source.is::<object_store::Error>();
            cause = source;
        }
        assert!(found_sdk);
        assert_eq!(cause.to_string(), "SDK source");
    }
}
