// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::pin::Pin;

use futures_util::Stream;
use futures_util::StreamExt;
use object_store::ObjectMeta;
use object_store::Result as ObjectStoreResult;
use qubit_fs::FsResult;
use qubit_fs::directory::ListFilter;
use qubit_fs::directory::ListScope;
use qubit_fs::error::FsError;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::DirEntry;
use qubit_fs::metadata::FileKind;
use qubit_fs::metadata::FileMetadata;
use qubit_fs::path::Path;
use qubit_fs::spi::AsyncDirectoryStreamSession;
use qubit_fs::spi::ResolvedListOptions;
use qubit_fs::spi::SpiFuture;

use crate::config::S3ContractConfig;
use crate::error_mapper;

/// Streams objects from an S3 namespace as facade directory entries.
///
/// The session rejects keys that escape the configured namespace and bounds
/// scanning so that filtered listings cannot consume unbounded provider data.
pub(crate) struct S3DirectoryStream {
    /// Configuration used to identify the provider-owned key prefix.
    config: S3ContractConfig,
    /// Requested logical directory scope within that prefix.
    scope: ListScope,
    /// Provider results consumed lazily as entries are requested.
    stream: Pin<Box<dyn Stream<Item = ObjectStoreResult<ObjectMeta>> + Send>>,
    /// Literal prefix applied to paths relative to the requested scope.
    filter: String,
    /// Whether returned entries include provider size metadata.
    include_metadata: bool,
    /// Number of provider results consumed, including filtered results.
    scanned: usize,
}

impl S3DirectoryStream {
    /// Creates a listing session over a provider stream and resolved options.
    ///
    /// The stream is consumed lazily by the async directory-stream interface,
    /// with a fixed scan budget across matching and filtered objects.
    pub(crate) fn new(
        config: S3ContractConfig,
        scope: ListScope,
        stream: Pin<Box<dyn Stream<Item = ObjectStoreResult<ObjectMeta>> + Send>>,
        filter: String,
        include_metadata: bool,
    ) -> Self {
        Self {
            config,
            scope,
            stream,
            filter,
            include_metadata,
            scanned: 0,
        }
    }
}
impl AsyncDirectoryStreamSession for S3DirectoryStream {
    /// Returns the next matching object, or an error for provider contract
    /// violations and listings that exceed the session scan budget.
    fn next_entry_async(&mut self) -> SpiFuture<'_, FsResult<Option<DirEntry>>> {
        Box::pin(async move {
            loop {
                let Some(value) = self.stream.next().await else {
                    return Ok(None);
                };
                self.scanned = self.scanned.saturating_add(1);
                if self.scanned > 1024 {
                    return Err(FsError::new(
                        FsErrorKind::ResourceLimitExceeded,
                        FsOperation::List,
                        "S3 fixture listing scan budget exceeded",
                    ));
                }
                let meta =
                    value.map_err(|error| error_mapper::map(error, FsOperation::List))?;
                let key = meta.location.to_string();
                let prefix = format!("{}/", self.config.prefix);
                let logical = key.strip_prefix(&prefix).ok_or_else(|| {
                    FsError::new(
                        FsErrorKind::ProviderContractViolation,
                        FsOperation::List,
                        "S3 listing escaped the configured namespace",
                    )
                })?;
                let root = self.scope.path().map_or("", |path| path.as_str());
                let Some(relative) = logical.strip_prefix(root) else {
                    continue;
                };
                if !relative.starts_with(&self.filter) {
                    continue;
                }
                let path = Path::parse_literal(logical).map_err(|error| {
                    FsError::with_source(
                        FsErrorKind::ProviderContractViolation,
                        FsOperation::List,
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

/// Converts supported listing filters into the literal prefix used by the S3
/// object-key listing implementation.
pub(crate) fn filter(options: &ResolvedListOptions) -> FsResult<String> {
    match options.options().filter() {
        Some(ListFilter::LiteralPrefix(prefix)) => Ok(prefix.clone()),
        None => Ok(String::new()),
        Some(ListFilter::Subtree(_)) => Err(FsError::new(
            FsErrorKind::InvalidOptions,
            FsOperation::List,
            "S3 fixture requires literal object-key prefixes",
        )),
    }
}

