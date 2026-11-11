// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stable identities and honest evidence states for random-access contracts.

mod common;

#[cfg(feature = "async")]
use std::future::Future;
#[cfg(feature = "async")]
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
#[cfg(feature = "async")]
use std::task::Context;
#[cfg(feature = "async")]
use std::task::Poll;
#[cfg(feature = "async")]
use std::task::Waker;

pub use ::qubit_fs_testkit;
use common::MemoryFixture;
#[cfg(feature = "async")]
use qubit_fs::AsyncFileSystem;
use qubit_fs::FileSystem;
use qubit_fs::FsResult;
use qubit_fs::error::FsError;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileKind;
use qubit_fs::metadata::FileMetadata;
use qubit_fs::metadata::FileSystemCapabilities;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::FileSystemId;
use qubit_fs::metadata::FileSystemInfo;
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs::metadata::OpenedFileInfo;
use qubit_fs::metadata::SymlinkPolicy;
use qubit_fs::path::Path;
use qubit_fs::path::PathConstraints;
use qubit_fs::path::PathSemantics;
#[cfg(feature = "async")]
use qubit_fs::spi::AsyncFileSystemSpi;
#[cfg(feature = "async")]
use qubit_fs::spi::AsyncRandomAccessReadSession;
use qubit_fs::spi::FileSystemSpi;
use qubit_fs::spi::OpenReaderRequest;
#[cfg(feature = "async")]
use qubit_fs::spi::OpenedAsyncRandomAccessReader;
use qubit_fs::spi::OpenedRandomAccessReader;
use qubit_fs::spi::ProviderOperation;
use qubit_fs::spi::ProviderOperations;
use qubit_fs::spi::ProviderProperties;
use qubit_fs::spi::RandomAccessReadSession;
#[cfg(feature = "async")]
use qubit_fs::spi::SpiFuture;
use qubit_fs::spi::StatRequest;
use qubit_fs::spi::StatResponse;
#[cfg(feature = "async")]
use qubit_fs_testkit::AsyncFileSystemContractSuite;
#[cfg(feature = "async")]
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;
use qubit_fs_testkit::FileSystemFixture;
#[cfg(feature = "async")]
use qubit_fs_testkit::FixtureFuture;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;

const PROVIDER_ID: &str = "random-access-test-fixture";

struct RandomProvider {
    current: Arc<Mutex<Arc<[u8]>>>,
    supported: bool,
    reopen_path: bool,
    limit: FileSystemLimit,
    open_error: Option<FsErrorKind>,
    reported_len: Option<u64>,
}

struct RandomSession {
    bytes: Arc<[u8]>,
    current: Arc<Mutex<Arc<[u8]>>>,
    reopen_path: bool,
}

impl RandomAccessReadSession for RandomSession {
    fn read_at(&mut self, offset: u64, output: &mut [u8]) -> FsResult<usize> {
        let bytes = if self.reopen_path {
            self.current.lock().expect("fixture state").clone()
        } else {
            self.bytes.clone()
        };
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        let count = output.len().min(1).min(bytes.len().saturating_sub(start));
        output[..count].copy_from_slice(&bytes[start..start + count]);
        Ok(count)
    }
}

impl FileSystemSpi for RandomProvider {
    fn properties(&self) -> ProviderProperties {
        let mut operations = ProviderOperations::new().with(ProviderOperation::OpenReader);
        if self.supported {
            operations = operations.with(ProviderOperation::OpenRandomAccessReader);
        }
        ProviderProperties::new(
            FileSystemInfo::new(
                FileSystemId::new(PROVIDER_ID).expect("valid ID"),
                "fixture",
                PathSemantics::Hierarchical,
            ),
            operations,
            FileSystemCapabilities::new().with_guaranteed(FileSystemCapability::Read),
            FileSystemLimits::unknown().with_max_read_range_bytes(self.limit),
            PathConstraints::absolute(),
            SymlinkPolicy::Reject,
        )
        .expect("valid provider properties")
    }

    fn stat(&self, request: StatRequest<'_>) -> FsResult<StatResponse> {
        let len = self.current.lock().expect("fixture state").len() as u64;
        Ok(StatResponse::new(
            request.path().clone(),
            FileMetadata::new(FileKind::File).with_len(Some(len)),
        ))
    }

    fn open_random_access_reader(&self, request: OpenReaderRequest<'_>) -> FsResult<OpenedRandomAccessReader> {
        if let Some(kind) = self.open_error {
            return Err(FsError::new(
                kind,
                FsOperation::OpenReader,
                "injected random-access open failure",
            ));
        }
        if !self.supported {
            return Err(FsError::new(
                FsErrorKind::UnsupportedOperation,
                FsOperation::OpenReader,
                "random access unsupported",
            ));
        }
        let bytes = self.current.lock().expect("fixture state").clone();
        let info = OpenedFileInfo::new(
            FileSystemId::new(PROVIDER_ID).expect("valid ID"),
            request.path().clone(),
        )
        .with_metadata(
            FileMetadata::new(FileKind::File).with_len(Some(self.reported_len.unwrap_or(bytes.len() as u64))),
        );
        Ok(OpenedRandomAccessReader::new(
            info,
            Box::new(RandomSession {
                bytes,
                current: self.current.clone(),
                reopen_path: self.reopen_path,
            }),
        ))
    }
}

struct RandomFixture {
    file_system: FileSystem,
    current: Arc<Mutex<Arc<[u8]>>>,
    supported: bool,
}

impl RandomFixture {
    fn new(supported: bool, reopen_path: bool, limit: FileSystemLimit) -> Self {
        Self::with_open_options(supported, reopen_path, limit, None, None)
    }

    fn with_open_options(
        supported: bool,
        reopen_path: bool,
        limit: FileSystemLimit,
        open_error: Option<FsErrorKind>,
        reported_len: Option<u64>,
    ) -> Self {
        let current = Arc::new(Mutex::new(Arc::from(&b""[..])));
        let file_system = FileSystem::from_spi(RandomProvider {
            current: current.clone(),
            supported,
            reopen_path,
            limit,
            open_error,
            reported_len,
        })
        .expect("provider facade initializes");
        Self {
            file_system,
            current,
            supported,
        }
    }
}

impl FileSystemFixture for RandomFixture {
    fn file_system(&self) -> &FileSystem {
        &self.file_system
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/{relative}"))
            .map_err(|error| qubit_fs_testkit::FixtureError::with_source("invalid fixture path", error))
    }
    fn teardown(&self) -> FixtureResult<()> {
        *self.current.lock().expect("fixture state") = Arc::from(&b""[..]);
        Ok(())
    }
    fn random_access_operation(&self) -> FixtureSupport<bool> {
        FixtureSupport::Supported(self.supported)
    }
    fn seed_file(&self, relative: &str, bytes: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        *self.current.lock().expect("fixture state") = Arc::from(bytes);
        Ok(FixtureSupport::Supported(self.path(relative)?))
    }
    fn replace_random_access_path(&self, _path: &Path, bytes: &[u8]) -> FixtureResult<FixtureSupport<()>> {
        *self.current.lock().expect("fixture state") = Arc::from(bytes);
        Ok(FixtureSupport::Supported(()))
    }
}

#[cfg(feature = "async")]
#[derive(Default)]
struct AsyncState {
    bytes: Arc<[u8]>,
    probe_armed: bool,
    probe_consumed: bool,
    inject_pending: bool,
    reopen_path: bool,
    teardown_calls: usize,
    disarm_calls: usize,
}

#[cfg(feature = "async")]
struct AsyncRandomProvider {
    state: Arc<Mutex<AsyncState>>,
    range_limit: FileSystemLimit,
}

#[cfg(feature = "async")]
struct AsyncRandomSession {
    bytes: Arc<[u8]>,
    state: Arc<Mutex<AsyncState>>,
}

#[cfg(feature = "async")]
impl AsyncRandomAccessReadSession for AsyncRandomSession {
    fn read_at_async<'a>(self: Pin<&'a mut Self>, offset: u64, output: &'a mut [u8]) -> SpiFuture<'a, FsResult<usize>> {
        let session = self.get_mut();
        let bytes = session.bytes.clone();
        let state = session.state.clone();
        Box::pin(async move {
            let (pending, reopen, current) = {
                let mut state = state.lock().expect("async fixture state");
                let pending = state.probe_armed && !state.probe_consumed && state.inject_pending;
                if pending {
                    state.probe_consumed = true;
                }
                (pending, state.reopen_path, state.bytes.clone())
            };
            if pending {
                std::future::poll_fn(|_| Poll::<()>::Pending).await;
            }
            let bytes = if reopen { current } else { bytes };
            let start = usize::try_from(offset).unwrap_or(usize::MAX);
            let count = output.len().min(1).min(bytes.len().saturating_sub(start));
            output[..count].copy_from_slice(&bytes[start..start + count]);
            Ok(count)
        })
    }
}

#[cfg(feature = "async")]
impl AsyncFileSystemSpi for AsyncRandomProvider {
    fn properties(&self) -> ProviderProperties {
        ProviderProperties::new(
            FileSystemInfo::new(
                FileSystemId::new(PROVIDER_ID).expect("valid ID"),
                "fixture",
                PathSemantics::Hierarchical,
            ),
            ProviderOperations::new()
                .with(ProviderOperation::OpenReader)
                .with(ProviderOperation::OpenRandomAccessReader),
            FileSystemCapabilities::new().with_guaranteed(FileSystemCapability::Read),
            FileSystemLimits::unknown().with_max_read_range_bytes(self.range_limit),
            PathConstraints::absolute(),
            SymlinkPolicy::Reject,
        )
        .expect("valid async provider properties")
    }
    fn stat<'a>(&'a self, request: StatRequest<'a>) -> SpiFuture<'a, FsResult<StatResponse>> {
        let len = self.state.lock().expect("async fixture state").bytes.len() as u64;
        Box::pin(async move {
            Ok(StatResponse::new(
                request.path().clone(),
                FileMetadata::new(FileKind::File).with_len(Some(len)),
            ))
        })
    }
    fn open_random_access_reader<'a>(
        &'a self,
        request: OpenReaderRequest<'a>,
    ) -> SpiFuture<'a, FsResult<OpenedAsyncRandomAccessReader>> {
        let state = self.state.clone();
        Box::pin(async move {
            let bytes = state.lock().expect("async fixture state").bytes.clone();
            let info = OpenedFileInfo::new(
                FileSystemId::new(PROVIDER_ID).expect("valid ID"),
                request.path().clone(),
            )
            .with_metadata(FileMetadata::new(FileKind::File).with_len(Some(bytes.len() as u64)));
            Ok(OpenedAsyncRandomAccessReader::new(
                info,
                Box::new(AsyncRandomSession {
                    bytes,
                    state: state.clone(),
                }),
            ))
        })
    }
}

#[cfg(feature = "async")]
struct AsyncRandomFixture {
    file_system: AsyncFileSystem,
    state: Arc<Mutex<AsyncState>>,
}

#[cfg(feature = "async")]
impl AsyncRandomFixture {
    fn new(inject_pending: bool, reopen_path: bool) -> Self {
        Self::with_limit(inject_pending, reopen_path, FileSystemLimit::Maximum(2))
    }

    fn with_limit(inject_pending: bool, reopen_path: bool, range_limit: FileSystemLimit) -> Self {
        let state = Arc::new(Mutex::new(AsyncState {
            inject_pending,
            reopen_path,
            ..AsyncState::default()
        }));
        let file_system = AsyncFileSystem::from_spi(AsyncRandomProvider {
            state: state.clone(),
            range_limit,
        })
        .expect("async provider facade initializes");
        Self { file_system, state }
    }
}

#[cfg(feature = "async")]
impl AsyncFileSystemFixture for AsyncRandomFixture {
    fn file_system(&self) -> &AsyncFileSystem {
        &self.file_system
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/{relative}"))
            .map_err(|error| qubit_fs_testkit::FixtureError::with_source("invalid async fixture path", error))
    }
    fn teardown(&self) -> FixtureFuture<'_, ()> {
        let state = self.state.clone();
        Box::pin(async move {
            let mut state = state.lock().expect("async fixture state");
            state.teardown_calls += 1;
            state.probe_armed = false;
            state.bytes = Arc::from(&b""[..]);
            Ok(())
        })
    }
    fn random_access_operation(&self) -> FixtureSupport<bool> {
        FixtureSupport::Supported(true)
    }
    fn seed_file<'a>(&'a self, relative: &'a str, bytes: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<Path>> {
        let state = self.state.clone();
        let path = self.path(relative);
        Box::pin(async move {
            state.lock().expect("async fixture state").bytes = Arc::from(bytes);
            Ok(FixtureSupport::Supported(path?))
        })
    }
    fn replace_random_access_path<'a>(
        &'a self,
        _path: &'a Path,
        bytes: &'a [u8],
    ) -> FixtureFuture<'a, FixtureSupport<()>> {
        self.state.lock().expect("async fixture state").bytes = Arc::from(bytes);
        Box::pin(async { Ok(FixtureSupport::Supported(())) })
    }
    fn arm_random_access_pending_read(&self, _path: &Path) -> FixtureResult<FixtureSupport<()>> {
        let mut state = self.state.lock().expect("async fixture state");
        state.probe_armed = true;
        state.probe_consumed = false;
        Ok(FixtureSupport::Supported(()))
    }
    fn disarm_random_access_pending_read(&self, _path: &Path) -> FixtureResult<()> {
        let mut state = self.state.lock().expect("async fixture state");
        state.probe_armed = false;
        state.disarm_calls += 1;
        if state.reopen_path {
            state.bytes = Arc::from(&b"UVWXYZ"[..]);
        }
        Ok(())
    }
}

#[cfg(feature = "async")]
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("contract suite should complete without a runtime"),
    }
}

struct DeclaredFixture {
    inner: MemoryFixture,
    declared: FixtureSupport<bool>,
    seed: bool,
}

impl FileSystemFixture for DeclaredFixture {
    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }
    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }
    fn random_access_operation(&self) -> FixtureSupport<bool> {
        self.declared.clone()
    }
    fn seed_file(&self, relative: &str, bytes: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        if self.seed {
            self.inner.seed_file(relative, bytes)
        } else {
            Ok(FixtureSupport::Unsupported)
        }
    }
}

struct ContradictoryDeclarationFixture {
    inner: RandomFixture,
}

impl FileSystemFixture for ContradictoryDeclarationFixture {
    fn file_system(&self) -> &FileSystem {
        &self.inner.file_system
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }
    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }
    fn random_access_operation(&self) -> FixtureSupport<bool> {
        FixtureSupport::Supported(false)
    }
    fn seed_file(&self, relative: &str, bytes: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        self.inner.seed_file(relative, bytes)
    }
    fn replace_random_access_path(&self, path: &Path, bytes: &[u8]) -> FixtureResult<FixtureSupport<()>> {
        self.inner.replace_random_access_path(path, bytes)
    }
}

struct PathFailureFixture {
    inner: MemoryFixture,
}

impl FileSystemFixture for PathFailureFixture {
    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }
    fn path(&self, _relative: &str) -> FixtureResult<Path> {
        Err(qubit_fs_testkit::FixtureError::new("random-access path unavailable"))
    }
    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }
    fn random_access_operation(&self) -> FixtureSupport<bool> {
        FixtureSupport::Supported(false)
    }
}

struct SeedFailureFixture {
    inner: RandomFixture,
}

impl FileSystemFixture for SeedFailureFixture {
    fn file_system(&self) -> &FileSystem {
        &self.inner.file_system
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }
    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }
    fn random_access_operation(&self) -> FixtureSupport<bool> {
        FixtureSupport::Supported(true)
    }
    fn seed_file(&self, _relative: &str, _bytes: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        Err(qubit_fs_testkit::FixtureError::new("random-access seed unavailable"))
    }
}

fn outcome(fixture: &dyn FileSystemFixture, id: ContractCheckId) -> ContractCheckOutcome {
    let mut suite = FileSystemContractSuite::new(fixture);
    let _ = suite.run_check(id);
    suite
        .report()
        .checks()
        .iter()
        .find(|check| check.id() == id)
        .expect("selected random-access check must be recorded")
        .outcome()
        .clone()
}

#[test]
fn test_random_access_checks_have_distinct_ids_and_default_to_skipped() {
    let fixture = MemoryFixture::new();
    for (id, name) in [
        (ContractCheckId::RandomAccessBasic, "read/random-access-basic"),
        (ContractCheckId::RandomAccessBounds, "read/random-access-bounds"),
        (ContractCheckId::RandomAccessRangeLimit, "read/random-access-range-limit"),
        (ContractCheckId::RandomAccessIdentity, "read/random-access-identity"),
    ] {
        assert_eq!(id.as_str(), name);
        assert_eq!(id.contract(), qubit_fs_testkit::FileSystemContract::Read);
        assert_eq!(
            outcome(&fixture, id),
            ContractCheckOutcome::SkippedOptional {
                reason: "fixture has no independent random-access operation declaration".to_owned(),
            }
        );
    }
}

struct NoReadRandomFixture {
    inner: MemoryFixture,
    declaration: FixtureSupport<bool>,
    declaration_calls: Arc<std::sync::atomic::AtomicUsize>,
}

impl FileSystemFixture for NoReadRandomFixture {
    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }
    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }
    fn random_access_operation(&self) -> FixtureSupport<bool> {
        self.declaration_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.declaration.clone()
    }
    fn seed_file(&self, _: &str, _: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        panic!("random-access seed called without Read")
    }
    fn replace_random_access_path(&self, _: &Path, _: &[u8]) -> FixtureResult<FixtureSupport<()>> {
        panic!("replacement hook called without Read")
    }
}

#[test]
fn test_random_access_checks_are_not_applicable_without_read() {
    for declaration in [
        FixtureSupport::Unsupported,
        FixtureSupport::Supported(false),
        FixtureSupport::Supported(true),
    ] {
        let declaration_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let fixture = NoReadRandomFixture {
            inner: MemoryFixture::without_operation_capabilities(),
            declaration,
            declaration_calls: declaration_calls.clone(),
        };
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(FileSystemContract::Read);
        assert!(run.failures().is_empty(), "{:?}", run.failures());
        let checks = run.report().checks();
        assert_eq!(
            checks
                .iter()
                .find(|check| check.id() == ContractCheckId::ReadBasic)
                .unwrap()
                .outcome(),
            &ContractCheckOutcome::RejectedAsExpected
        );
        for id in [
            ContractCheckId::RandomAccessBasic,
            ContractCheckId::RandomAccessBounds,
            ContractCheckId::RandomAccessRangeLimit,
            ContractCheckId::RandomAccessIdentity,
        ] {
            let check = checks.iter().find(|check| check.id() == id).unwrap();
            match check.outcome() {
                ContractCheckOutcome::NotApplicable { reason } => assert!(!reason.trim().is_empty()),
                other => panic!("{id} should be not applicable, got {other:?}"),
            }
        }
        assert_eq!(declaration_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}

#[test]
fn test_random_access_declared_support_is_verified_or_rejected() {
    let unsupported = DeclaredFixture {
        inner: MemoryFixture::new(),
        declared: FixtureSupport::Supported(false),
        seed: false,
    };
    assert_eq!(
        outcome(&unsupported, ContractCheckId::RandomAccessBasic),
        ContractCheckOutcome::RejectedAsExpected
    );

    let declared_without_seed = DeclaredFixture {
        inner: MemoryFixture::new(),
        declared: FixtureSupport::Supported(true),
        seed: false,
    };
    assert!(matches!(
        outcome(&declared_without_seed, ContractCheckId::RandomAccessBasic),
        ContractCheckOutcome::Unverified { .. }
    ));

    let declared_with_seed = DeclaredFixture {
        inner: MemoryFixture::new(),
        declared: FixtureSupport::Supported(true),
        seed: true,
    };
    let mut suite = FileSystemContractSuite::new(&declared_with_seed);
    let run = suite.run_check(ContractCheckId::RandomAccessBasic);
    assert!(
        !run.failures().is_empty(),
        "declared operation unsupported by SPI must fail"
    );
}

#[test]
fn test_random_access_false_declaration_rejects_provider_support() {
    let fixture = ContradictoryDeclarationFixture {
        inner: RandomFixture::new(true, false, FileSystemLimit::Unknown),
    };
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::RandomAccessBasic);
    assert_eq!(run.failures().len(), 1);
    assert!(
        run.failures()[0]
            .message()
            .contains("after declaring the operation unsupported")
    );
}

#[test]
fn test_random_access_false_declaration_reports_unexpected_provider_error() {
    let fixture = ContradictoryDeclarationFixture {
        inner: RandomFixture::with_open_options(true, false, FileSystemLimit::Unknown, Some(FsErrorKind::Io), None),
    };
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::RandomAccessBasic);
    assert_eq!(run.failures().len(), 1);
    assert!(run.failures()[0].message().contains("unexpected error"));
}

#[test]
fn test_random_access_unsupported_declaration_reports_path_setup_failure() {
    let fixture = PathFailureFixture {
        inner: MemoryFixture::new(),
    };
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::RandomAccessBounds);
    assert_eq!(run.failures().len(), 1);
    assert!(
        run.failures()[0]
            .message()
            .contains("random-access path preparation failed")
    );
}

#[test]
fn test_random_access_seed_setup_failure_is_reported() {
    let fixture = SeedFailureFixture {
        inner: RandomFixture::new(true, false, FileSystemLimit::Unknown),
    };
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::RandomAccessBasic);
    assert_eq!(run.failures().len(), 1);
    assert!(
        run.failures()[0]
            .message()
            .contains("random-access seed preparation failed")
    );
}

struct LimitSeedUnavailableFixture {
    inner: RandomFixture,
    fail: bool,
}

impl FileSystemFixture for LimitSeedUnavailableFixture {
    fn file_system(&self) -> &FileSystem {
        &self.inner.file_system
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }
    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }
    fn random_access_operation(&self) -> FixtureSupport<bool> {
        FixtureSupport::Supported(true)
    }
    fn seed_file(&self, relative: &str, bytes: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        if relative.contains("random-access-limit") {
            if self.fail {
                return Err(qubit_fs_testkit::FixtureError::new("limit seed unavailable"));
            }
            return Ok(FixtureSupport::Unsupported);
        }
        self.inner.seed_file(relative, bytes)
    }
}

#[test]
fn test_random_access_limit_seed_unavailability_is_unverified_and_cleanup_completes() {
    let fixture = LimitSeedUnavailableFixture {
        inner: RandomFixture::new(true, false, FileSystemLimit::Maximum(10)),
        fail: false,
    };
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::RandomAccessRangeLimit);
    assert!(run.failures().is_empty(), "{:?}", run.failures());
    assert!(run.cleanup().completed());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::Unverified { .. }
    ));
    assert!(!run.requirements_satisfied_with(&[ContractCheckId::RandomAccessBounds]));

    let fixture = LimitSeedUnavailableFixture {
        inner: RandomFixture::new(true, false, FileSystemLimit::Maximum(10)),
        fail: true,
    };
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::RandomAccessRangeLimit);
    assert_eq!(run.failures().len(), 1);
    assert!(run.failures()[0].message().contains("limit seed preparation failed"));
    assert!(run.cleanup().completed());
}

#[test]
fn test_random_access_reads_bounds_and_object_identity() {
    let conforming = RandomFixture::new(true, false, FileSystemLimit::Unknown);
    for id in [
        ContractCheckId::RandomAccessBasic,
        ContractCheckId::RandomAccessBounds,
        ContractCheckId::RandomAccessIdentity,
    ] {
        assert_eq!(outcome(&conforming, id), ContractCheckOutcome::Passed, "{id}");
    }

    let bounded = RandomFixture::new(true, false, FileSystemLimit::Maximum(2));
    assert_eq!(
        outcome(&bounded, ContractCheckId::RandomAccessBounds),
        ContractCheckOutcome::Passed
    );

    let tiny_bounded = RandomFixture::new(true, false, FileSystemLimit::Maximum(1));
    for id in [
        ContractCheckId::RandomAccessBasic,
        ContractCheckId::RandomAccessBounds,
        ContractCheckId::RandomAccessIdentity,
    ] {
        assert_eq!(outcome(&tiny_bounded, id), ContractCheckOutcome::Passed, "{id}");
    }

    let incorrect_length = RandomFixture::with_open_options(true, false, FileSystemLimit::Unknown, None, Some(7));
    assert!(matches!(
        outcome(&incorrect_length, ContractCheckId::RandomAccessBasic),
        ContractCheckOutcome::Failed { .. }
    ));

    for maximum in [5, 6, 10, 63] {
        let fixture = RandomFixture::new(true, false, FileSystemLimit::Maximum(maximum));
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::RandomAccessBounds);
        assert_eq!(run.failures().len(), 0, "Maximum({maximum}): {:?}", run.failures());
        assert_eq!(
            run.report().checks()[0].outcome(),
            &ContractCheckOutcome::Passed,
            "Maximum({maximum})"
        );
        assert!(run.requirements_satisfied_with(&[ContractCheckId::RandomAccessBounds]));
        assert_eq!(outcome(&fixture, ContractCheckId::RandomAccessRangeLimit), ContractCheckOutcome::Passed);
    }

    for maximum in [64, u64::MAX] {
        let fixture = RandomFixture::new(true, false, FileSystemLimit::Maximum(maximum));
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::RandomAccessBounds);
        assert_eq!(run.failures().len(), 0, "Maximum({maximum}): {:?}", run.failures());
        assert_eq!(run.report().checks()[0].outcome(), &ContractCheckOutcome::Passed);
        assert!(run.requirements_satisfied_with(&[ContractCheckId::RandomAccessBounds]));
        let mut range_suite = FileSystemContractSuite::new(&fixture);
        let range = range_suite.run_check(ContractCheckId::RandomAccessRangeLimit);
        assert_eq!(range.report().checks().len(), 1);
        assert_eq!(range.report().checks()[0].id(), ContractCheckId::RandomAccessRangeLimit);
        assert!(matches!(range.report().checks()[0].outcome(), ContractCheckOutcome::SkippedOptional { .. }));
    }

    for maximum in [1, 2, 5, 6, 10, 63] {
        let fixture = RandomFixture::new(true, false, FileSystemLimit::Maximum(maximum));
        assert_eq!(outcome(&fixture, ContractCheckId::RandomAccessBounds), ContractCheckOutcome::Passed);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::RandomAccessRangeLimit);
        assert_eq!(run.report().checks().len(), 1);
        assert_eq!(run.report().checks()[0].id(), ContractCheckId::RandomAccessRangeLimit);
        assert_eq!(run.report().checks()[0].outcome(), &ContractCheckOutcome::Passed);
    }
    for limit in [FileSystemLimit::Unknown, FileSystemLimit::Unbounded, FileSystemLimit::NotApplicable] {
        let fixture = RandomFixture::new(true, false, limit);
        assert!(matches!(outcome(&fixture, ContractCheckId::RandomAccessRangeLimit), ContractCheckOutcome::NotApplicable { .. }));
    }

    let reopens_path = RandomFixture::new(true, true, FileSystemLimit::Unknown);
    assert!(matches!(
        outcome(&reopens_path, ContractCheckId::RandomAccessIdentity),
        ContractCheckOutcome::Failed { .. }
    ));
}

#[cfg(feature = "async")]
#[test]
fn test_async_random_access_bounds_accept_finite_ranges() {
    for maximum in [1, 2, 5, 6, 10, 63] {
        let fixture = AsyncRandomFixture::with_limit(true, false, FileSystemLimit::Maximum(maximum));
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(ContractCheckId::RandomAccessBounds));
        assert!(
            run.failures().is_empty(),
            "async Maximum({maximum}): {:?}",
            run.failures()
        );
        assert_eq!(run.report().checks()[0].id(), ContractCheckId::RandomAccessBounds);
        assert_eq!(
            run.report().checks()[0].outcome(),
            &ContractCheckOutcome::Passed,
            "Maximum({maximum})"
        );
        assert!(run.requirements_satisfied_with(&[ContractCheckId::RandomAccessBounds]));
        let mut range_suite = AsyncFileSystemContractSuite::new(&fixture);
        let range_run = ready(range_suite.run_check(ContractCheckId::RandomAccessRangeLimit));
        assert_eq!(range_run.report().checks().len(), 1);
        assert_eq!(range_run.report().checks()[0].id(), ContractCheckId::RandomAccessRangeLimit);
        assert_eq!(range_run.report().checks()[0].outcome(), &ContractCheckOutcome::Passed);
    }

    for maximum in [64, u64::MAX] {
        let fixture = AsyncRandomFixture::with_limit(true, false, FileSystemLimit::Maximum(maximum));
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(ContractCheckId::RandomAccessBounds));
        assert!(
            run.failures().is_empty(),
            "async Maximum({maximum}): {:?}",
            run.failures()
        );
        assert_eq!(run.report().checks()[0].outcome(), &ContractCheckOutcome::Passed);
        let mut range_suite = AsyncFileSystemContractSuite::new(&fixture);
        let range = ready(range_suite.run_check(ContractCheckId::RandomAccessRangeLimit));
        assert_eq!(range.report().checks().len(), 1);
        assert_eq!(range.report().checks()[0].id(), ContractCheckId::RandomAccessRangeLimit);
        assert!(matches!(range.report().checks()[0].outcome(), ContractCheckOutcome::SkippedOptional { .. }));
    }

    for limit in [FileSystemLimit::Unknown, FileSystemLimit::Unbounded, FileSystemLimit::NotApplicable] {
        let fixture = AsyncRandomFixture::with_limit(true, false, limit);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(ContractCheckId::RandomAccessRangeLimit));
        assert!(matches!(run.report().checks()[0].outcome(), ContractCheckOutcome::NotApplicable { .. }));
    }
}

#[cfg(feature = "async")]
#[test]
fn test_async_random_access_checks_accept_one_byte_ranges() {
    let fixture = AsyncRandomFixture::with_limit(true, false, FileSystemLimit::Maximum(1));
    for id in [
        ContractCheckId::RandomAccessBasic,
        ContractCheckId::RandomAccessBounds,
        ContractCheckId::RandomAccessRangeLimit,
        ContractCheckId::RandomAccessIdentity,
    ] {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(id));
        assert!(run.failures().is_empty(), "async {id}: {:?}", run.failures());
        assert_eq!(
            run.report().checks()[0].outcome(),
            &ContractCheckOutcome::Passed,
            "{id}"
        );
    }
}

#[cfg(feature = "async")]
#[test]
fn test_async_random_access_identity_detects_path_reopening() {
    let fixture = AsyncRandomFixture::with_limit(true, true, FileSystemLimit::Maximum(1));
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let run = ready(suite.run_check(ContractCheckId::RandomAccessIdentity));
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::Failed { .. }
    ));
}

#[cfg(feature = "async")]
#[test]
fn test_random_access_cancel_releases_futures_and_reuses_reader() {
    let fixture = AsyncRandomFixture::new(true, false);
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let run = ready(suite.run_check(ContractCheckId::RandomAccessCancel));
    assert!(
        run.failures().is_empty(),
        "cancel contract should complete without failures"
    );
    assert_eq!(run.report().checks()[0].id(), ContractCheckId::RandomAccessCancel);
    assert_eq!(run.report().checks()[0].outcome(), &ContractCheckOutcome::Passed);
    let state = fixture.state.lock().expect("async fixture state");
    assert_eq!(state.disarm_calls, 1);
    assert_eq!(state.teardown_calls, 1);
    assert!(!state.probe_armed, "teardown leaves no pending-read probe armed");
}

#[cfg(feature = "async")]
struct AsyncNoReadRandomFixture {
    inner: common::AsyncMemoryFixture,
    declaration: FixtureSupport<bool>,
    declaration_calls: Arc<std::sync::atomic::AtomicUsize>,
}

#[cfg(feature = "async")]
impl AsyncFileSystemFixture for AsyncNoReadRandomFixture {
    fn file_system(&self) -> &qubit_fs::AsyncFileSystem {
        self.inner.file_system()
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }
    fn teardown(&self) -> FixtureFuture<'_, ()> {
        self.inner.teardown()
    }
    fn random_access_operation(&self) -> FixtureSupport<bool> {
        self.declaration_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.declaration.clone()
    }
    fn seed_file<'a>(&'a self, _: &'a str, _: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<Path>> {
        panic!("random-access seed called without Read")
    }
    fn replace_random_access_path<'a>(&'a self, _: &'a Path, _: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<()>> {
        panic!("replacement hook called without Read")
    }
    fn arm_random_access_pending_read(&self, _: &Path) -> FixtureResult<FixtureSupport<()>> {
        panic!("cancellation hook called without Read")
    }
    fn disarm_random_access_pending_read(&self, _: &Path) -> FixtureResult<()> {
        panic!("cancellation cleanup called without Read")
    }
}

#[cfg(feature = "async")]
#[test]
fn test_async_random_access_checks_are_not_applicable_without_read() {
    for declaration in [
        FixtureSupport::Unsupported,
        FixtureSupport::Supported(false),
        FixtureSupport::Supported(true),
    ] {
        let declaration_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let fixture = AsyncNoReadRandomFixture {
            inner: common::AsyncMemoryFixture::without_operation_capabilities(),
            declaration,
            declaration_calls: declaration_calls.clone(),
        };
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_contract(FileSystemContract::Read));
        assert!(run.failures().is_empty(), "{:?}", run.failures());
        let checks = run.report().checks();
        assert_eq!(
            checks
                .iter()
                .find(|check| check.id() == ContractCheckId::ReadBasic)
                .unwrap()
                .outcome(),
            &ContractCheckOutcome::RejectedAsExpected
        );
        for id in [
            ContractCheckId::RandomAccessBasic,
            ContractCheckId::RandomAccessBounds,
            ContractCheckId::RandomAccessRangeLimit,
            ContractCheckId::RandomAccessIdentity,
            ContractCheckId::RandomAccessCancel,
        ] {
            let check = checks.iter().find(|check| check.id() == id).unwrap();
            match check.outcome() {
                ContractCheckOutcome::NotApplicable { reason } => assert!(!reason.trim().is_empty()),
                other => panic!("{id} should be not applicable, got {other:?}"),
            }
        }
        assert_eq!(declaration_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}

#[cfg(feature = "async")]
#[test]
fn test_random_access_cancel_reuses_reader_with_one_byte_range_limit() {
    let fixture = AsyncRandomFixture::with_limit(true, false, FileSystemLimit::Maximum(1));
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let run = ready(suite.run_check(ContractCheckId::RandomAccessCancel));
    assert!(
        run.failures().is_empty(),
        "one-byte random-access limit should support cancellation and reuse: {:?}",
        run.failures()
    );
    assert_eq!(run.report().checks()[0].outcome(), &ContractCheckOutcome::Passed);
}

#[cfg(feature = "async")]
#[test]
fn test_random_access_cancel_reports_unverified_and_detects_rebinding() {
    let no_pending = AsyncRandomFixture::new(false, false);
    let mut suite = AsyncFileSystemContractSuite::new(&no_pending);
    let run = ready(suite.run_check(ContractCheckId::RandomAccessCancel));
    assert_eq!(run.report().checks()[0].id(), ContractCheckId::RandomAccessCancel);
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::Unverified { .. }
    ));
    assert_eq!(no_pending.state.lock().expect("state").disarm_calls, 1);

    let reopens_path = AsyncRandomFixture::new(true, true);
    let mut suite = AsyncFileSystemContractSuite::new(&reopens_path);
    let run = ready(suite.run_check(ContractCheckId::RandomAccessCancel));
    assert_eq!(run.report().checks()[0].id(), ContractCheckId::RandomAccessCancel);
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::Failed { .. }
    ));
    let state = reopens_path.state.lock().expect("state");
    assert_eq!(state.disarm_calls, 1);
    assert_eq!(state.teardown_calls, 1);
    assert!(!state.probe_armed);
}
