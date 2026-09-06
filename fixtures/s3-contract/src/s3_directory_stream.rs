use std::pin::Pin;

use futures_util::Stream;
use futures_util::StreamExt;
use object_store::ObjectMeta;
use qubit_fs::FsResult;
use qubit_fs::directory::ListFilter;
use qubit_fs::metadata::DirEntry;
use qubit_fs::metadata::FileKind;
use qubit_fs::metadata::FileMetadata;
use qubit_fs::path::Path;
use qubit_fs::spi::AsyncDirectoryStreamSession;
use qubit_fs::spi::SpiFuture;

use crate::config::S3ContractConfig;
use crate::error_mapper;

pub(crate) struct S3DirectoryStream {
    config: S3ContractConfig,
    stream: Pin<Box<dyn Stream<Item = object_store::Result<ObjectMeta>> + Send>>,
    filter: String,
    include_metadata: bool,
    scanned: usize,
}

impl S3DirectoryStream {
    pub(crate) fn new(
        config: S3ContractConfig,
        _root: Path,
        stream: Pin<Box<dyn Stream<Item = object_store::Result<ObjectMeta>> + Send>>,
        filter: String,
        include_metadata: bool,
    ) -> Self {
        Self {
            config,
            stream,
            filter,
            include_metadata,
            scanned: 0,
        }
    }
}

impl AsyncDirectoryStreamSession for S3DirectoryStream {
    fn next_entry_async(&mut self) -> SpiFuture<'_, FsResult<Option<DirEntry>>> {
        Box::pin(async move {
            loop {
                let Some(value) = self.stream.next().await else {
                    return Ok(None);
                };
                self.scanned = self.scanned.saturating_add(1);
                if self.scanned > 1024 {
                    return Err(qubit_fs::error::FsError::new(
                        qubit_fs::error::FsErrorKind::ResourceLimitExceeded,
                        qubit_fs::error::FsOperation::List,
                        "S3 fixture listing scan budget exceeded",
                    ));
                }
                let meta = value.map_err(|error| {
                    error_mapper::map(error, qubit_fs::error::FsOperation::List)
                })?;
                let key = meta.location.to_string();
                let prefix = self.config.prefix.trim_end_matches('/');
                let logical = key
                    .strip_prefix(prefix)
                    .and_then(|value| value.strip_prefix('/'))
                    .unwrap_or(key.as_str());
                if !logical.starts_with(&self.filter) {
                    continue;
                }
                let path = Path::parse_literal(logical).map_err(|error| {
                    qubit_fs::error::FsError::with_source(
                        qubit_fs::error::FsErrorKind::InvalidPath,
                        qubit_fs::error::FsOperation::List,
                        "S3 object key cannot be represented by the facade path",
                        error,
                    )
                })?;
                let metadata = self
                    .include_metadata
                    .then(|| FileMetadata::new(FileKind::Object).with_len(Some(meta.size)));
                let mut entry = DirEntry::new(path, FileKind::Object);
                entry.metadata = metadata;
                return Ok(Some(entry));
            }
        })
    }
}

pub(crate) fn filter(options: &qubit_fs::spi::ResolvedListOptions) -> FsResult<String> {
    match options.options().filter() {
        Some(ListFilter::LiteralPrefix(prefix)) => Ok(prefix.clone()),
        None => Ok(String::new()),
        Some(ListFilter::Subtree(_)) => Err(qubit_fs::error::FsError::new(
            qubit_fs::error::FsErrorKind::InvalidOptions,
            qubit_fs::error::FsOperation::List,
            "S3 fixture requires literal object-key prefixes",
        )),
    }
}
