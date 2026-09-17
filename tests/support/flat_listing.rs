// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Flat fixture with an independent set model and injected listing defects.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::Mutex;

#[cfg(feature = "async")]
use qubit_fs::AsyncFileSystem;
use qubit_fs::FileSystem;
use qubit_fs::FsError;
use qubit_fs::FsResult;
use qubit_fs::Path;
use qubit_fs::directory::ListFilter;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::DirEntry;
use qubit_fs::metadata::FileKind;
use qubit_fs::metadata::FileMetadata;
use qubit_fs::metadata::FileSystemCapabilities;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::FileSystemId;
use qubit_fs::metadata::FileSystemInfo;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs::metadata::SymlinkPolicy;
use qubit_fs::path::PathConstraints;
use qubit_fs::path::PathSemantics;
#[cfg(feature = "async")]
use qubit_fs::spi::AsyncDirectoryStreamSession;
#[cfg(feature = "async")]
use qubit_fs::spi::AsyncFileSystemSpi;
use qubit_fs::spi::DirectoryStreamSpi;
use qubit_fs::spi::FileSystemSpi;
use qubit_fs::spi::ListRequest;
#[cfg(feature = "async")]
use qubit_fs::spi::OpenedAsyncDirectoryStream;
use qubit_fs::spi::OpenedDirectoryStream;
use qubit_fs::spi::ProviderOperation;
use qubit_fs::spi::ProviderOperations;
use qubit_fs::spi::ProviderProperties;
#[cfg(feature = "async")]
use qubit_fs::spi::SpiFuture;
use qubit_fs::spi::StatRequest;
use qubit_fs::spi::StatResponse;
#[cfg(feature = "async")]
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureError;
#[cfg(feature = "async")]
use qubit_fs_testkit::FixtureFuture;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;

/// One observable provider defect, or a conforming listing.
#[derive(Clone, Copy)]
pub enum Fault {
    None,
    OmitExisting,
    Duplicate,
    IgnoreFilter,
    ComponentPrefix,
    PathFailure,
    SeedFailure,
    SnapshotFailure,
    OpenFailure,
    StreamFailure,
    SeedUnsupported,
    SnapshotIncomplete,
}

/// Fixture model containing preexisting keys as well as contract seeds.
pub struct FlatFixture {
    filesystem: FileSystem,
    #[cfg(feature = "async")]
    asynchronous: AsyncFileSystem,
    keys: Arc<Mutex<BTreeSet<String>>>,
    snapshot: bool,
    fault: Fault,
}

impl FlatFixture {
    /// Creates an isolated provider and an independent namespace observation.
    pub fn new(fault: Fault, snapshot: bool) -> Self {
        let keys = Arc::new(Mutex::new(BTreeSet::from(["preexisting".to_owned()])));
        let provider = Provider {
            keys: Arc::clone(&keys),
            fault,
        };
        Self {
            filesystem: FileSystem::from_spi(provider.clone()).unwrap(),
            #[cfg(feature = "async")]
            asynchronous: AsyncFileSystem::from_spi(provider).unwrap(),
            keys,
            snapshot,
            fault,
        }
    }

    /// Maps one key without involving the facade.
    fn key(relative: &str) -> FixtureResult<Path> {
        Path::parse_literal(relative).map_err(|error| FixtureError::with_source("invalid fixture key", error))
    }

    /// Seeds the independent model directly.
    fn seed(&self, relative: &str) -> FixtureResult<FixtureSupport<Path>> {
        if matches!(self.fault, Fault::SeedFailure) {
            return Err(FixtureError::new("injected seed failure"));
        }
        if matches!(self.fault, Fault::SeedUnsupported) {
            return Ok(FixtureSupport::Unsupported);
        }
        let path = Self::key(relative)?;
        self.keys.lock().unwrap().insert(relative.to_owned());
        Ok(FixtureSupport::Supported(path))
    }

    /// Snapshots the model rather than consulting provider list results.
    fn snapshot(&self) -> FixtureResult<FixtureSupport<Vec<Path>>> {
        if matches!(self.fault, Fault::SnapshotFailure) {
            return Err(FixtureError::new("injected snapshot failure"));
        }
        if matches!(self.fault, Fault::SnapshotIncomplete) {
            return Ok(FixtureSupport::Supported(Vec::new()));
        }
        if !self.snapshot {
            return Ok(FixtureSupport::Unsupported);
        }
        self.keys
            .lock()
            .unwrap()
            .iter()
            .map(|key| Self::key(key))
            .collect::<FixtureResult<Vec<_>>>()
            .map(FixtureSupport::Supported)
    }
}

impl FileSystemFixture for FlatFixture {
    /// Returns the synchronous facade under test.
    fn file_system(&self) -> &FileSystem {
        &self.filesystem
    }
    /// Resolves a fixture key without provider I/O.
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        if matches!(self.fault, Fault::PathFailure) {
            return Err(FixtureError::new("injected path failure"));
        }
        Self::key(relative)
    }
    /// Prepares test data outside the facade.
    fn seed_file(&self, relative: &str, _: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        self.seed(relative)
    }
    /// Provides complete expected keys independently.
    fn snapshot_namespace_paths(&self) -> FixtureResult<FixtureSupport<Vec<Path>>> {
        self.snapshot()
    }
    /// Removes all fixture-owned keys out of band.
    fn teardown(&self) -> FixtureResult<()> {
        self.keys.lock().unwrap().clear();
        Ok(())
    }
}

#[cfg(feature = "async")]
impl AsyncFileSystemFixture for FlatFixture {
    /// Returns the asynchronous facade under test.
    fn file_system(&self) -> &AsyncFileSystem {
        &self.asynchronous
    }
    /// Resolves a fixture key without provider I/O.
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        if matches!(self.fault, Fault::PathFailure) {
            return Err(FixtureError::new("injected path failure"));
        }
        Self::key(relative)
    }
    /// Prepares test data independently when polled.
    fn seed_file<'a>(&'a self, relative: &'a str, _: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<Path>> {
        Box::pin(async move { self.seed(relative) })
    }
    /// Reads the independent complete model when polled.
    fn snapshot_namespace_paths(&self) -> FixtureFuture<'_, FixtureSupport<Vec<Path>>> {
        Box::pin(async move { self.snapshot() })
    }
    /// Clears fixture-owned model entries.
    fn teardown(&self) -> FixtureFuture<'_, ()> {
        Box::pin(async move {
            self.keys.lock().unwrap().clear();
            Ok(())
        })
    }
}

/// Provider dispatch deliberately separate from fixture snapshots.
#[derive(Clone)]
struct Provider {
    keys: Arc<Mutex<BTreeSet<String>>>,
    fault: Fault,
}

impl Provider {
    /// Exposes flat list support with no unrelated guarantees.
    fn snapshot(&self) -> ProviderProperties {
        ProviderProperties::new(
            FileSystemInfo::new(
                FileSystemId::new("flat-test").unwrap(),
                "flat-test",
                PathSemantics::ObjectKey,
            ),
            ProviderOperations::new()
                .with(ProviderOperation::Stat)
                .with(ProviderOperation::List),
            FileSystemCapabilities::new().with_guaranteed(FileSystemCapability::List),
            FileSystemLimits::unknown(),
            PathConstraints::either(),
            SymlinkPolicy::Reject,
        )
        .unwrap()
    }

    /// Selects actual results, optionally injecting an independent defect.
    fn list_entries(&self, request: ListRequest<'_>) -> FsResult<Entries> {
        if matches!(self.fault, Fault::OpenFailure) {
            return Err(FsError::new(
                FsErrorKind::PermissionDenied,
                FsOperation::List,
                "injected open failure",
            ));
        }
        let root = request.scope().path().map_or("", |path| path.as_str());
        let filter = match request.options().options().filter() {
            Some(ListFilter::LiteralPrefix(filter)) => filter.as_str(),
            _ => "",
        };
        let mut keys = self
            .keys
            .lock()
            .unwrap()
            .iter()
            .filter(|key| {
                if matches!(self.fault, Fault::OmitExisting) && key.as_str() == "preexisting" {
                    return false;
                }
                let Some(relative) = key.strip_prefix(root) else {
                    return false;
                };
                if matches!(self.fault, Fault::ComponentPrefix)
                    && !root.is_empty()
                    && !relative.is_empty()
                    && !relative.starts_with('/')
                {
                    return false;
                }
                matches!(self.fault, Fault::IgnoreFilter) || relative.starts_with(filter)
            })
            .map(|key| DirEntry::new(Path::parse_literal(key).unwrap(), FileKind::File))
            .collect::<Vec<_>>();
        if request.options().options().include_metadata() {
            for entry in &mut keys {
                entry.metadata = Some(FileMetadata::new(FileKind::File));
            }
        }
        if matches!(self.fault, Fault::Duplicate)
            && let Some(first) = keys.first().cloned()
        {
            keys.push(first);
        }
        Ok(Entries {
            entries: keys.into_iter(),
            fail: matches!(self.fault, Fault::StreamFailure),
        })
    }
}

impl FileSystemSpi for Provider {
    /// Returns the immutable contract snapshot.
    fn properties(&self) -> ProviderProperties {
        self.snapshot()
    }
    /// Listing must not probe metadata implicitly.
    fn stat(&self, _: StatRequest<'_>) -> FsResult<StatResponse> {
        Err(FsError::new(
            FsErrorKind::UnsupportedOperation,
            FsOperation::Stat,
            "unused",
        ))
    }
    /// Opens a fixed set of provider results.
    fn list(&self, request: ListRequest<'_>) -> FsResult<OpenedDirectoryStream> {
        Ok(OpenedDirectoryStream::new(Box::new(self.list_entries(request)?)))
    }
}

#[cfg(feature = "async")]
impl AsyncFileSystemSpi for Provider {
    /// Returns the same contract as the synchronous provider.
    fn properties(&self) -> ProviderProperties {
        self.snapshot()
    }
    /// Rejects unexpected metadata probes.
    fn stat<'a>(&'a self, _: StatRequest<'a>) -> SpiFuture<'a, FsResult<StatResponse>> {
        Box::pin(async {
            Err(FsError::new(
                FsErrorKind::UnsupportedOperation,
                FsOperation::Stat,
                "unused",
            ))
        })
    }
    /// Opens the fixed result set only when polled.
    fn list<'a>(&'a self, request: ListRequest<'a>) -> SpiFuture<'a, FsResult<OpenedAsyncDirectoryStream>> {
        Box::pin(async move { Ok(OpenedAsyncDirectoryStream::new(Box::new(self.list_entries(request)?))) })
    }
}

/// Iterates the provider result set independently of fixture observation.
struct Entries {
    entries: std::vec::IntoIter<DirEntry>,
    fail: bool,
}
impl DirectoryStreamSpi for Entries {
    /// Advances one entry.
    fn next_entry(&mut self) -> FsResult<Option<DirEntry>> {
        if self.fail {
            return Err(FsError::new(
                FsErrorKind::PermissionDenied,
                FsOperation::List,
                "injected stream failure",
            ));
        }
        Ok(self.entries.next())
    }
}
#[cfg(feature = "async")]
impl AsyncDirectoryStreamSession for Entries {
    /// Advances one entry when polled.
    fn next_entry_async(&mut self) -> SpiFuture<'_, FsResult<Option<DirEntry>>> {
        Box::pin(async move { self.next_entry() })
    }
}
