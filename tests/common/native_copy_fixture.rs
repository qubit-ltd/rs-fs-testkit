// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Minimal provider whose native copy does not depend on facade Read/Write.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

#[cfg(feature = "async")]
use qubit_fs::AsyncFileSystem;
use qubit_fs::FileSystem;
use qubit_fs::copy::CopyConflictPolicy;
use qubit_fs::copy::CopyMethod;
use qubit_fs::copy::CopyOutcome;
use qubit_fs::copy::CopyStats;
use qubit_fs::error::FsError;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::AchievedAtomicity;
use qubit_fs::metadata::FileKind;
use qubit_fs::metadata::FileMetadata;
use qubit_fs::metadata::FileSystemCapabilities;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::FileSystemId;
use qubit_fs::metadata::FileSystemInfo;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs::metadata::FileSystemProperties;
use qubit_fs::metadata::SymlinkPolicy;
use qubit_fs::path::Path;
use qubit_fs::path::PathConstraints;
use qubit_fs::path::PathSemantics;
#[cfg(feature = "async")]
use qubit_fs::spi::AsyncFileSystemSpi;
use qubit_fs::spi::CopyAttempt;
use qubit_fs::spi::CopyRequest;
use qubit_fs::spi::FileSystemSpi;
use qubit_fs::spi::OpenReaderRequest;
use qubit_fs::spi::OpenWriterRequest;
use qubit_fs::spi::ProviderOperation;
use qubit_fs::spi::ProviderOperations;
use qubit_fs::spi::ProviderProperties;
use qubit_fs::spi::SpiCopyFailure;
#[cfg(feature = "async")]
use qubit_fs::spi::SpiFuture;
use qubit_fs::spi::StatRequest;
use qubit_fs::spi::StatResponse;
#[cfg(feature = "async")]
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::CopyFixtureCase;
use qubit_fs_testkit::CopyScenario;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureError;
#[cfg(feature = "async")]
use qubit_fs_testkit::FixtureFuture;
use qubit_fs_testkit::FixturePreparation;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;

#[derive(Default)]
struct State {
    files: HashMap<String, Vec<u8>>,
    cleanup_snapshot: HashMap<String, Vec<u8>>,
    last_source: Option<String>,
    last_target: Option<String>,
    seed_supported: bool,
    observation_supported: bool,
    native_calls: usize,
    reader_calls: usize,
    writer_calls: usize,
    policies: Vec<CopyConflictPolicy>,
    fail_copy: bool,
    wrong_content: bool,
}

struct NativeSpi {
    state: Arc<Mutex<State>>,
    read: bool,
    write: bool,
}

impl FileSystemSpi for NativeSpi {
    fn properties(&self) -> ProviderProperties {
        let mut caps = FileSystemCapabilities::new().with_guaranteed(FileSystemCapability::Copy);
        if self.read {
            caps = caps.with_guaranteed(FileSystemCapability::Read);
        }
        if self.write {
            caps = caps.with_guaranteed(FileSystemCapability::Write);
        }
        let props = FileSystemProperties::new(
            FileSystemInfo::new(
                FileSystemId::new("native-copy-test").expect("valid id"),
                "native-copy-test",
                PathSemantics::Hierarchical,
            ),
            caps,
            FileSystemLimits::unknown(),
            PathConstraints::absolute(),
            SymlinkPolicy::Reject,
        )
        .expect("valid properties");
        let mut ops = ProviderOperations::new()
            .with(ProviderOperation::Stat)
            .with(ProviderOperation::TryCopy);
        if self.read {
            ops = ops.with(ProviderOperation::OpenReader);
        }
        if self.write {
            ops = ops.with(ProviderOperation::OpenWriter);
        }
        ProviderProperties::new(
            props.info().clone(),
            ops,
            props.capabilities(),
            *props.limits(),
            props.path_constraints().clone(),
            props.symlink_policy(),
        )
        .expect("valid provider properties")
    }
    fn stat(&self, request: StatRequest<'_>) -> qubit_fs::error::FsResult<StatResponse> {
        let state = self.state.lock().expect("state lock");
        match state.files.get(request.path().as_str()) {
            Some(bytes) => Ok(StatResponse::new(
                request.path().clone(),
                FileMetadata::new(FileKind::File).with_len(Some(bytes.len() as u64)),
            )),
            None => Err(FsError::new(FsErrorKind::NotFound, FsOperation::Stat, "missing")),
        }
    }
    fn open_reader(&self, request: OpenReaderRequest<'_>) -> qubit_fs::error::FsResult<qubit_fs::spi::OpenedReader> {
        self.state.lock().expect("state lock").reader_calls += 1;
        Err(FsError::new(
            FsErrorKind::PermissionDenied,
            FsOperation::OpenReader,
            "native fixture denies stream fallback access",
        )
        .with_path(request.path().clone()))
    }
    fn open_writer(&self, request: OpenWriterRequest<'_>) -> qubit_fs::error::FsResult<qubit_fs::spi::OpenedWriter> {
        self.state.lock().expect("state lock").writer_calls += 1;
        Err(FsError::new(
            FsErrorKind::PermissionDenied,
            FsOperation::OpenWriter,
            "native fixture denies stream fallback access",
        )
        .with_path(request.path().clone())
        .with_effect_state(qubit_fs::error::FsEffectState::Unchanged))
    }
    fn try_copy(&self, request: CopyRequest<'_>) -> Result<CopyAttempt, SpiCopyFailure> {
        let mut s = self.state.lock().expect("state lock");
        s.native_calls += 1;
        let policy = request.options().options().conflict();
        s.policies.push(policy);
        if s.fail_copy {
            return Err(SpiCopyFailure::new(
                FsError::new(
                    FsErrorKind::PermissionDenied,
                    FsOperation::Copy,
                    "injected native copy error",
                ),
                qubit_fs::copy::CopyFailureState::Unchanged,
                CopyStats::default(),
            ));
        }
        let Some(bytes) = s.files.get(request.source().as_str()).cloned() else {
            return Err(SpiCopyFailure::new(
                FsError::new(FsErrorKind::NotFound, FsOperation::Copy, "source absent"),
                qubit_fs::copy::CopyFailureState::Unchanged,
                CopyStats::default(),
            ));
        };
        let exists = s.files.contains_key(request.target().as_str());
        if exists && policy == CopyConflictPolicy::Fail {
            return Err(SpiCopyFailure::new(
                FsError::new(FsErrorKind::AlreadyExists, FsOperation::Copy, "target exists"),
                qubit_fs::copy::CopyFailureState::Unchanged,
                CopyStats::default(),
            ));
        }
        if exists && policy == CopyConflictPolicy::Skip {
            return Ok(CopyAttempt::Completed(CopyOutcome::new(
                CopyStats {
                    skipped: 1,
                    ..CopyStats::default()
                },
                CopyMethod::Native,
                AchievedAtomicity::NonAtomic,
            )));
        }
        let out = if s.wrong_content {
            b"wrong native payload".to_vec()
        } else {
            bytes.clone()
        };
        s.files.insert(request.target().as_str().to_owned(), out);
        Ok(CopyAttempt::Completed(CopyOutcome::new(
            CopyStats {
                files: 1,
                bytes: bytes.len() as u64,
                overwritten: u64::from(exists),
                ..CopyStats::default()
            },
            CopyMethod::Native,
            AchievedAtomicity::NonAtomic,
        )))
    }
}

#[cfg(feature = "async")]
struct AsyncNativeSpi(NativeSpi);
#[cfg(feature = "async")]
impl AsyncFileSystemSpi for AsyncNativeSpi {
    fn properties(&self) -> ProviderProperties {
        self.0.properties()
    }
    fn stat<'a>(&'a self, req: StatRequest<'a>) -> SpiFuture<'a, qubit_fs::error::FsResult<StatResponse>> {
        Box::pin(async move { self.0.stat(req) })
    }
    fn open_reader<'a>(
        &'a self,
        req: OpenReaderRequest<'a>,
    ) -> SpiFuture<'a, qubit_fs::error::FsResult<qubit_fs::spi::OpenedAsyncReader>> {
        Box::pin(async move {
            self.0.state.lock().expect("state lock").reader_calls += 1;
            Err(FsError::new(
                FsErrorKind::PermissionDenied,
                FsOperation::OpenReader,
                "native async fixture denies stream fallback access",
            )
            .with_path(req.path().clone()))
        })
    }
    fn open_writer<'a>(
        &'a self,
        req: OpenWriterRequest<'a>,
    ) -> SpiFuture<'a, qubit_fs::error::FsResult<qubit_fs::spi::OpenedAsyncWriter>> {
        Box::pin(async move {
            self.0.state.lock().expect("state lock").writer_calls += 1;
            Err(FsError::new(
                FsErrorKind::PermissionDenied,
                FsOperation::OpenWriter,
                "native async fixture denies stream fallback access",
            )
            .with_path(req.path().clone())
            .with_effect_state(qubit_fs::error::FsEffectState::Unchanged))
        })
    }
    fn try_copy<'a>(&'a self, req: CopyRequest<'a>) -> SpiFuture<'a, Result<CopyAttempt, SpiCopyFailure>> {
        Box::pin(async move { self.0.try_copy(req) })
    }
}

/// Independent seed/read fixture for native-copy capability combinations.
pub(crate) struct NativeCopyFixture {
    fs: FileSystem,
    state: Arc<Mutex<State>>,
}
impl NativeCopyFixture {
    pub(crate) fn new(read: bool, write: bool) -> Self {
        Self::with_fixture_support(read, write, true, true)
    }
    pub(crate) fn with_fixture_support(
        read: bool,
        write: bool,
        seed_supported: bool,
        observation_supported: bool,
    ) -> Self {
        let state = Arc::new(Mutex::new(State {
            seed_supported,
            observation_supported,
            ..State::default()
        }));
        let fs = FileSystem::from_spi(NativeSpi {
            state: Arc::clone(&state),
            read,
            write,
        })
        .expect("native copy SPI properties");
        Self { fs, state }
    }
    pub(crate) fn native_calls(&self) -> usize {
        self.state.lock().expect("state lock").native_calls
    }
    pub(crate) fn reader_writer_calls(&self) -> (usize, usize) {
        let s = self.state.lock().expect("state lock");
        (s.reader_calls, s.writer_calls)
    }
    pub(crate) fn policies(&self) -> Vec<CopyConflictPolicy> {
        self.state.lock().expect("state lock").policies.clone()
    }
    pub(crate) fn inject_copy_error(&self) {
        self.state.lock().expect("state lock").fail_copy = true;
    }
    pub(crate) fn inject_wrong_content(&self) {
        self.state.lock().expect("state lock").wrong_content = true;
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.state.lock().expect("state lock").files.is_empty()
    }
    pub(crate) fn file_system(&self) -> &FileSystem {
        &self.fs
    }
    pub(crate) fn cleanup_snapshot(&self) -> HashMap<String, Vec<u8>> {
        self.state.lock().expect("state lock").cleanup_snapshot.clone()
    }
    pub(crate) fn last_paths(&self) -> (String, String) {
        let s = self.state.lock().expect("state lock");
        (
            s.last_source.clone().expect("copy source recorded"),
            s.last_target.clone().expect("copy target recorded"),
        )
    }
}
impl FileSystemFixture for NativeCopyFixture {
    fn file_system(&self) -> &FileSystem {
        &self.fs
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/native-copy/{relative}")).map_err(|e| FixtureError::new(e.to_string()))
    }
    fn seed_file(&self, relative: &str, bytes: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        let p = self.path(relative)?;
        let mut state = self.state.lock().expect("state lock");
        if !state.seed_supported {
            return Ok(FixtureSupport::Unsupported);
        }
        state.files.insert(p.as_str().to_owned(), bytes.to_vec());
        Ok(FixtureSupport::Supported(p))
    }
    fn read_file(&self, path: &Path) -> FixtureResult<FixtureSupport<Vec<u8>>> {
        let state = self.state.lock().expect("state lock");
        if !state.observation_supported {
            return Ok(FixtureSupport::Unsupported);
        }
        Ok(state
            .files
            .get(path.as_str())
            .cloned()
            .map_or(FixtureSupport::Unsupported, FixtureSupport::Supported))
    }
    fn prepare_copy(
        &self,
        scenario: CopyScenario,
        source: &str,
        target: &str,
        bytes: &[u8],
    ) -> FixtureResult<FixturePreparation<CopyFixtureCase>> {
        if scenario != CopyScenario::Basic && scenario != CopyScenario::Conflict {
            return Ok(FixturePreparation::Unavailable {
                reason: "native fixture supports file copy scenarios only".into(),
            });
        }
        let src = self.path(source)?;
        let dst = self.path(target)?;
        {
            let mut state = self.state.lock().expect("state lock");
            state.last_source = Some(src.as_str().to_owned());
            state.last_target = Some(dst.as_str().to_owned());
        }
        let FixtureSupport::Supported(src) = self.seed_file(source, bytes)? else {
            return Ok(FixturePreparation::Unavailable {
                reason: "native fixture cannot seed the copy source".into(),
            });
        };
        let dst = if scenario == CopyScenario::Conflict {
            let FixtureSupport::Supported(dst) = self.seed_file(target, b"existing")? else {
                return Ok(FixturePreparation::Unavailable {
                    reason: "native fixture cannot seed the copy target".into(),
                });
            };
            dst
        } else {
            dst
        };
        Ok(FixturePreparation::Ready(CopyFixtureCase::new(
            src,
            dst,
            qubit_fs::copy::CopyOptions::file(),
        )))
    }
    fn teardown(&self) -> FixtureResult<()> {
        let mut s = self.state.lock().expect("state lock");
        s.cleanup_snapshot = s.files.clone();
        s.files.clear();
        Ok(())
    }
}

#[cfg(feature = "async")]
pub(crate) struct AsyncNativeCopyFixture {
    fs: AsyncFileSystem,
    state: Arc<Mutex<State>>,
}
#[cfg(feature = "async")]
impl AsyncNativeCopyFixture {
    pub(crate) fn new(read: bool, write: bool) -> Self {
        Self::with_fixture_support(read, write, true, true)
    }
    pub(crate) fn with_fixture_support(
        read: bool,
        write: bool,
        seed_supported: bool,
        observation_supported: bool,
    ) -> Self {
        let state = Arc::new(Mutex::new(State {
            seed_supported,
            observation_supported,
            ..State::default()
        }));
        let fs = AsyncFileSystem::from_spi(AsyncNativeSpi(NativeSpi {
            state: Arc::clone(&state),
            read,
            write,
        }))
        .expect("native async copy SPI properties");
        Self { fs, state }
    }
    pub(crate) fn native_calls(&self) -> usize {
        self.state.lock().expect("state lock").native_calls
    }
    pub(crate) fn reader_writer_calls(&self) -> (usize, usize) {
        let s = self.state.lock().expect("state lock");
        (s.reader_calls, s.writer_calls)
    }
    pub(crate) fn policies(&self) -> Vec<CopyConflictPolicy> {
        self.state.lock().expect("state lock").policies.clone()
    }
    pub(crate) fn inject_copy_error(&self) {
        self.state.lock().expect("state lock").fail_copy = true;
    }
    pub(crate) fn inject_wrong_content(&self) {
        self.state.lock().expect("state lock").wrong_content = true;
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.state.lock().expect("state lock").files.is_empty()
    }
    pub(crate) fn cleanup_snapshot(&self) -> HashMap<String, Vec<u8>> {
        self.state.lock().expect("state lock").cleanup_snapshot.clone()
    }
    pub(crate) fn last_paths(&self) -> (String, String) {
        let s = self.state.lock().expect("state lock");
        (
            s.last_source.clone().expect("copy source recorded"),
            s.last_target.clone().expect("copy target recorded"),
        )
    }
}
#[cfg(feature = "async")]
impl AsyncFileSystemFixture for AsyncNativeCopyFixture {
    fn file_system(&self) -> &AsyncFileSystem {
        &self.fs
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/native-copy/{relative}")).map_err(|e| FixtureError::new(e.to_string()))
    }
    fn seed_file<'a>(&'a self, relative: &'a str, bytes: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<Path>> {
        Box::pin(async move {
            let p = self.path(relative)?;
            let mut state = self.state.lock().expect("state lock");
            if !state.seed_supported {
                return Ok(FixtureSupport::Unsupported);
            }
            state.files.insert(p.as_str().to_owned(), bytes.to_vec());
            Ok(FixtureSupport::Supported(p))
        })
    }
    fn read_file<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<Vec<u8>>> {
        Box::pin(async move {
            let state = self.state.lock().expect("state lock");
            if !state.observation_supported {
                return Ok(FixtureSupport::Unsupported);
            }
            Ok(state
                .files
                .get(path.as_str())
                .cloned()
                .map_or(FixtureSupport::Unsupported, FixtureSupport::Supported))
        })
    }
    fn prepare_copy<'a>(
        &'a self,
        scenario: CopyScenario,
        source: &'a str,
        target: &'a str,
        bytes: &'a [u8],
    ) -> FixtureFuture<'a, FixturePreparation<CopyFixtureCase>> {
        Box::pin(async move {
            if scenario != CopyScenario::Basic && scenario != CopyScenario::Conflict {
                return Ok(FixturePreparation::Unavailable {
                    reason: "native fixture supports file copy scenarios only".into(),
                });
            }
            let src = self.path(source)?;
            let dst = self.path(target)?;
            {
                let mut state = self.state.lock().expect("state lock");
                state.last_source = Some(src.as_str().to_owned());
                state.last_target = Some(dst.as_str().to_owned());
            }
            let FixtureSupport::Supported(src) = self.seed_file(source, bytes).await? else {
                return Ok(FixturePreparation::Unavailable {
                    reason: "native fixture cannot seed the copy source".into(),
                });
            };
            let dst = if scenario == CopyScenario::Conflict {
                let FixtureSupport::Supported(dst) = self.seed_file(target, b"existing").await? else {
                    return Ok(FixturePreparation::Unavailable {
                        reason: "native fixture cannot seed the copy target".into(),
                    });
                };
                dst
            } else {
                dst
            };
            Ok(FixturePreparation::Ready(CopyFixtureCase::new(
                src,
                dst,
                qubit_fs::copy::CopyOptions::file(),
            )))
        })
    }
    fn teardown(&self) -> FixtureFuture<'_, ()> {
        Box::pin(async move {
            let mut s = self.state.lock().expect("state lock");
            s.cleanup_snapshot = s.files.clone();
            s.files.clear();
            Ok(())
        })
    }
}
