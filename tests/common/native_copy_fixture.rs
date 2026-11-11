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
    discard_nonempty: bool,
    native_source: Option<Vec<u8>>,
    native_same_path: bool,
    native_target_exists: bool,
    native_prepare_unsupported: bool,
    native_prepare_error: bool,
    read_unsupported: bool,
    read_error: bool,
    read_error_after_first: bool,
    read_calls: usize,
    exists_unsupported: bool,
    exists_error: bool,
    wrong_method: bool,
    wrong_stats: bool,
    mutate_source: bool,
}

struct NativeSpi {
    state: Arc<Mutex<State>>,
    read: bool,
    write: bool,
    limits: FileSystemLimits,
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
            self.limits,
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
        let out = if s.discard_nonempty && !bytes.is_empty() {
            Vec::new()
        } else if s.wrong_content {
            b"wrong native payload".to_vec()
        } else {
            bytes.clone()
        };
        s.files.insert(request.target().as_str().to_owned(), out);
        if s.mutate_source {
            s.files
                .insert(request.source().as_str().to_owned(), b"mutated source".to_vec());
        }
        let method = if s.wrong_method {
            CopyMethod::Streamed
        } else {
            CopyMethod::Native
        };
        let bytes_count = if s.wrong_stats { 0 } else { bytes.len() as u64 };
        Ok(CopyAttempt::Completed(CopyOutcome::new(
            CopyStats {
                files: 1,
                bytes: bytes_count,
                overwritten: u64::from(exists),
                ..CopyStats::default()
            },
            method,
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
        Self::with_limits(
            read,
            write,
            seed_supported,
            observation_supported,
            FileSystemLimits::unknown(),
        )
    }
    pub(crate) fn with_limits(
        read: bool,
        write: bool,
        seed_supported: bool,
        observation_supported: bool,
        limits: FileSystemLimits,
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
            limits,
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
    pub(crate) fn inject_discard_nonempty(&self) {
        self.state.lock().expect("state lock").discard_nonempty = true;
    }
    pub(crate) fn inject_native_fault(&self, name: &str) {
        let mut state = self.state.lock().expect("state lock");
        match name {
            "wrong_method" => state.wrong_method = true,
            "wrong_stats" => state.wrong_stats = true,
            "mutate_source" => state.mutate_source = true,
            "target_exists" => state.native_target_exists = true,
            "same_path" => state.native_same_path = true,
            "prepare_unsupported" => state.native_prepare_unsupported = true,
            "prepare_error" => state.native_prepare_error = true,
            "read_unsupported" => state.read_unsupported = true,
            "read_error" => state.read_error = true,
            "read_error_after_first" => state.read_error_after_first = true,
            "exists_unsupported" => state.exists_unsupported = true,
            "exists_error" => state.exists_error = true,
            "empty_source" => state.native_source = Some(Vec::new()),
            "large_source" => state.native_source = Some(vec![b'x'; 64 * 1024 + 1]),
            "copy_error" => state.fail_copy = true,
            "wrong_content" => state.wrong_content = true,
            _ => panic!("unknown native fault"),
        }
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
        let mut state = self.state.lock().expect("state lock");
        state.read_calls += 1;
        if state.read_error || (state.read_error_after_first && state.read_calls > 1) {
            return Err(FixtureError::new("injected read observation error"));
        }
        if state.read_unsupported {
            return Ok(FixtureSupport::Unsupported);
        }
        if !state.observation_supported {
            return Ok(FixtureSupport::Unsupported);
        }
        Ok(state
            .files
            .get(path.as_str())
            .cloned()
            .map_or(FixtureSupport::Unsupported, FixtureSupport::Supported))
    }
    fn exists_out_of_band(&self, path: &Path) -> FixtureResult<FixtureSupport<bool>> {
        let state = self.state.lock().expect("state lock");
        if state.exists_error {
            return Err(FixtureError::new("injected existence observation error"));
        }
        if state.exists_unsupported {
            return Ok(FixtureSupport::Unsupported);
        }
        Ok(FixtureSupport::Supported(state.files.contains_key(path.as_str())))
    }
    fn copy_fast_path_case(&self, method: CopyMethod) -> FixtureResult<FixtureSupport<CopyFixtureCase>> {
        if method != CopyMethod::Native {
            return Ok(FixtureSupport::Unsupported);
        }
        let state = self.state.lock().expect("state lock");
        if state.native_prepare_error {
            return Err(FixtureError::new("injected native preparation error"));
        }
        if state.native_prepare_unsupported {
            return Ok(FixtureSupport::Unsupported);
        }
        let source = self.path("native-source")?;
        let target = if state.native_same_path {
            source.clone()
        } else {
            self.path("native-target")?
        };
        if state.native_target_exists {
            drop(state);
            let _ = self.seed_file("native-target", b"existing")?;
        } else {
            drop(state);
        }
        let source_bytes = self
            .state
            .lock()
            .expect("state lock")
            .native_source
            .clone()
            .unwrap_or_else(|| b"native copy bytes".to_vec());
        {
            let mut state = self.state.lock().expect("state lock");
            state.last_source = Some(source.as_str().to_owned());
            state.last_target = Some(target.as_str().to_owned());
        }
        match self.seed_file("native-source", &source_bytes)? {
            FixtureSupport::Supported(source) => Ok(FixtureSupport::Supported(CopyFixtureCase::new(
                source,
                target,
                qubit_fs::copy::CopyOptions::file(),
            ))),
            FixtureSupport::Unsupported => Ok(FixtureSupport::Unsupported),
        }
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
        Self::with_limits(
            read,
            write,
            seed_supported,
            observation_supported,
            FileSystemLimits::unknown(),
        )
    }
    pub(crate) fn with_limits(
        read: bool,
        write: bool,
        seed_supported: bool,
        observation_supported: bool,
        limits: FileSystemLimits,
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
            limits,
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
    pub(crate) fn inject_native_fault(&self, name: &str) {
        let mut state = self.state.lock().expect("state lock");
        match name {
            "wrong_method" => state.wrong_method = true,
            "wrong_stats" => state.wrong_stats = true,
            "mutate_source" => state.mutate_source = true,
            "target_exists" => state.native_target_exists = true,
            "same_path" => state.native_same_path = true,
            "prepare_unsupported" => state.native_prepare_unsupported = true,
            "prepare_error" => state.native_prepare_error = true,
            "read_unsupported" => state.read_unsupported = true,
            "read_error" => state.read_error = true,
            "read_error_after_first" => state.read_error_after_first = true,
            "exists_unsupported" => state.exists_unsupported = true,
            "exists_error" => state.exists_error = true,
            "empty_source" => state.native_source = Some(Vec::new()),
            "large_source" => state.native_source = Some(vec![b'x'; 64 * 1024 + 1]),
            "copy_error" => state.fail_copy = true,
            "wrong_content" => state.wrong_content = true,
            _ => panic!("unknown native fault"),
        }
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
            let mut state = self.state.lock().expect("state lock");
            state.read_calls += 1;
            if state.read_error || (state.read_error_after_first && state.read_calls > 1) {
                return Err(FixtureError::new("injected read observation error"));
            }
            if state.read_unsupported {
                return Ok(FixtureSupport::Unsupported);
            }
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
    fn exists_out_of_band<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<bool>> {
        Box::pin(async move {
            let state = self.state.lock().expect("state lock");
            if state.exists_error {
                return Err(FixtureError::new("injected existence observation error"));
            }
            if state.exists_unsupported {
                return Ok(FixtureSupport::Unsupported);
            }
            Ok(FixtureSupport::Supported(state.files.contains_key(path.as_str())))
        })
    }
    fn copy_fast_path_case<'a>(&'a self, method: CopyMethod) -> FixtureFuture<'a, FixtureSupport<CopyFixtureCase>> {
        Box::pin(async move {
            if method != CopyMethod::Native {
                return Ok(FixtureSupport::Unsupported);
            }
            let (prepare_error, prepare_unsupported, same_path, target_exists, source_bytes) = {
                let state = self.state.lock().expect("state lock");
                (
                    state.native_prepare_error,
                    state.native_prepare_unsupported,
                    state.native_same_path,
                    state.native_target_exists,
                    state
                        .native_source
                        .clone()
                        .unwrap_or_else(|| b"native copy bytes".to_vec()),
                )
            };
            if prepare_error {
                return Err(FixtureError::new("injected native preparation error"));
            }
            if prepare_unsupported {
                return Ok(FixtureSupport::Unsupported);
            }
            let source = self.path("native-source")?;
            let target = if same_path {
                source.clone()
            } else {
                self.path("native-target")?
            };
            if target_exists {
                let _ = self.seed_file("native-target", b"existing").await?;
            }
            {
                let mut state = self.state.lock().expect("state lock");
                state.last_source = Some(source.as_str().to_owned());
                state.last_target = Some(target.as_str().to_owned());
            }
            match self.seed_file("native-source", &source_bytes).await? {
                FixtureSupport::Supported(source) => Ok(FixtureSupport::Supported(CopyFixtureCase::new(
                    source,
                    target,
                    qubit_fs::copy::CopyOptions::file(),
                ))),
                FixtureSupport::Unsupported => Ok(FixtureSupport::Unsupported),
            }
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
