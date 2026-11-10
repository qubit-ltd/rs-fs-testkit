// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Independently checks asynchronous random-access sessions.

use std::future::Future;
use std::pin::pin;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::read::ReadOptions;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixtureSupport;

const SEED: &[u8] = b"abcdef";

impl AsyncFileSystemContractSuite<'_> {
    /// Executes one independently prepared random-access check.
    ///
    /// # Parameters
    ///
    /// * `id` — the stable identity of the contract check being executed or
    ///   recorded.
    pub(super) async fn execute_random_access(
        &mut self,
        id: ContractCheckId,
    ) -> Result<ContractCheckOutcome, ContractFailure> {
        match self.fixture.random_access_operation() {
            FixtureSupport::Unsupported => {
                return Ok(ContractCheckOutcome::SkippedOptional {
                    reason: "fixture has no independent random-access operation declaration".to_owned(),
                });
            }
            FixtureSupport::Supported(false) => {
                let path = self
                    .fixture
                    .path(&self.context.relative_name("random-access"))
                    .map_err(|error| {
                        ContractFailure::with_source("random-access path preparation failed", error).at(id)
                    })?;
                match self
                    .fixture
                    .file_system()
                    .open_random_access_reader(&path, ReadOptions::default())
                    .await
                {
                    Err(error) if error.kind() == FsErrorKind::UnsupportedOperation => {
                        return Ok(ContractCheckOutcome::RejectedAsExpected);
                    }
                    Err(error) => {
                        return Err(ContractFailure::with_source(
                            "undeclared random-access operation returned an unexpected error",
                            error,
                        )
                        .at(id));
                    }
                    Ok(_) => {
                        return Err(ContractFailure::message_only(
                            "provider opened a random-access reader after declaring the operation unsupported",
                        )
                        .at(id));
                    }
                }
            }
            FixtureSupport::Supported(true) => {}
        }
        let range_limit = self.context.properties().limits().max_read_range_bytes();
        let relative = self.context.relative_name("random-access");
        let path = match self
            .fixture
            .seed_file(&relative, SEED)
            .await
            .map_err(|error| ContractFailure::with_source("random-access seed preparation failed", error).at(id))?
        {
            FixtureSupport::Supported(path) => path,
            FixtureSupport::Unsupported => {
                return Ok(ContractCheckOutcome::Unverified {
                    reason: "provider declares random access but fixture could not prepare the seed".to_owned(),
                });
            }
        };
        self.context.record_created(path.clone());
        if id == ContractCheckId::RandomAccessCancel {
            return self.execute_random_access_cancel(&path, range_limit, id).await;
        }
        let mut reader = self
            .fixture
            .file_system()
            .open_random_access_reader(&path, ReadOptions::default())
            .await
            .map_err(|error| ContractFailure::with_source("declared random-access open failed", error).at(id))?;
        if reader.len() != SEED.len() as u64 {
            return Err(ContractFailure::message_only("random-access open-time length did not match the seed").at(id));
        }
        match id {
            ContractCheckId::RandomAccessBasic => {
                verify_sample(&mut reader, 4, b"ef", range_limit, id).await?;
                verify_sample(&mut reader, 0, b"ab", range_limit, id).await?;
                verify_sample(&mut reader, 4, b"ef", range_limit, id).await?;
            }
            ContractCheckId::RandomAccessBounds => {
                let mut empty = [];
                if reader
                    .read_at(u64::MAX, &mut empty)
                    .await
                    .map_err(|error| ContractFailure::with_source("empty random-access read failed", error).at(id))?
                    != 0
                {
                    return Err(ContractFailure::message_only("empty random-access read returned bytes").at(id));
                }
                let mut sample = [0; 2];
                if reader
                    .read_at(6, &mut sample)
                    .await
                    .map_err(|error| ContractFailure::with_source("EOF random-access read failed", error).at(id))?
                    != 0
                {
                    return Err(ContractFailure::message_only("random-access read past EOF returned bytes").at(id));
                }
                let mut clipped = [0; 2];
                let clipped_count = reader.read_at(5, &mut clipped).await.map_err(|error| {
                    ContractFailure::with_source("open-time length clipping read failed", error).at(id)
                })?;
                if clipped_count != 1 || clipped[0] != b'f' {
                    return Err(ContractFailure::message_only(
                        "random-access request was not clipped to the open-time length",
                    )
                    .at(id));
                }
                let overflow = reader
                    .read_at(u64::MAX, &mut sample)
                    .await
                    .expect_err("overflow must be rejected");
                if overflow.kind() != FsErrorKind::InvalidOptions {
                    return Err(ContractFailure::message_only(
                        "overflowing random-access range did not return InvalidOptions",
                    )
                    .at(id));
                }
                if let FileSystemLimit::Maximum(maximum) = self.context.properties().limits().max_read_range_bytes() {
                    let Some((accepted, rejected)) =
                        crate::internal::limit_probe_plan::finite_probe(FileSystemLimit::Maximum(maximum), 64)
                    else {
                        return Ok(ContractCheckOutcome::SkippedOptional {
                            reason: "finite random-access range limit exceeds the 64-byte probe budget".to_owned(),
                        });
                    };
                    if rejected > SEED.len() as u64 {
                        return Ok(ContractCheckOutcome::SkippedOptional {
                            reason: "seed is too short to probe the declared random-access range limit".to_owned(),
                        });
                    }
                    if accepted > 0 {
                        verify_sample(&mut reader, 0, &SEED[..accepted as usize], range_limit, id).await?;
                    }
                    let mut rejected_buffer = vec![0; rejected as usize];
                    let error = reader
                        .read_at(0, &mut rejected_buffer)
                        .await
                        .expect_err("range above finite random-access limit must fail");
                    if error.kind() != FsErrorKind::ResourceLimitExceeded || error.operation() != FsOperation::Read {
                        return Err(ContractFailure::message_only(
                            "range above finite random-access limit returned the wrong error",
                        )
                        .at(id));
                    }
                }
                verify_sample(&mut reader, 0, b"ab", range_limit, id).await?;
            }
            ContractCheckId::RandomAccessIdentity => {
                let FixtureSupport::Supported(()) = self
                    .fixture
                    .replace_random_access_path(&path, b"WXYZ")
                    .await
                    .map_err(|error| {
                        ContractFailure::with_source("random-access path replacement failed", error).at(id)
                    })?
                else {
                    return Ok(ContractCheckOutcome::SkippedOptional {
                        reason: "fixture cannot replace the random-access path independently".to_owned(),
                    });
                };
                verify_sample(&mut reader, 0, b"ab", range_limit, id).await?;
                let mut replacement = self
                    .fixture
                    .file_system()
                    .open_random_access_reader(&path, ReadOptions::default())
                    .await
                    .map_err(|error| {
                        ContractFailure::with_source("replacement random-access open failed", error).at(id)
                    })?;
                verify_sample(&mut replacement, 0, b"WX", range_limit, id).await?;
            }
            ContractCheckId::RandomAccessCancel => {
                unreachable!("cancellation returned before opening the ordinary reader")
            }
            _ => unreachable!("random-access dispatch selected a different check"),
        }
        Ok(ContractCheckOutcome::Passed)
    }

    /// Cancels an injected pending read and verifies the reader remains usable.
    ///
    /// # Parameters
    ///
    /// * `path` — the filesystem path whose state or operation result is being
    ///   checked.
    /// * `id` — the stable identity of the contract check being executed or
    ///   recorded.
    async fn execute_random_access_cancel(
        &mut self,
        path: &qubit_fs::path::Path,
        range_limit: FileSystemLimit,
        id: ContractCheckId,
    ) -> Result<ContractCheckOutcome, ContractFailure> {
        let FixtureSupport::Supported(()) = self
            .fixture
            .arm_random_access_pending_read(path)
            .map_err(|error| ContractFailure::with_source("pending random-access probe arm failed", error).at(id))?
        else {
            return Ok(ContractCheckOutcome::SkippedOptional {
                reason: "fixture cannot inject a pending random-access read".to_owned(),
            });
        };
        let fixture = self.fixture;
        let probe_path = path;
        let mut probe_guard = crate::internal::probe_disarm_guard::ProbeDisarmGuard::new(
            move || fixture.disarm_random_access_pending_read(probe_path),
            &mut self.context.run.failures,
        );
        let mut reader = fixture
            .file_system()
            .open_random_access_reader(path, ReadOptions::default())
            .await
            .map_err(|error| ContractFailure::with_source("declared random-access open failed", error).at(id))?;
        if reader.len() != SEED.len() as u64 {
            return Err(ContractFailure::message_only("random-access open-time length did not match the seed").at(id));
        }
        let probe_len = range_limit
            .maximum()
            .and_then(|maximum| usize::try_from(maximum).ok())
            .map_or(2, |maximum| maximum.min(2));
        if probe_len == 0 {
            return Ok(ContractCheckOutcome::Unverified {
                reason: "random-access range limit prevents a non-empty pending-read probe".to_owned(),
            });
        }
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        {
            let mut output = vec![0; probe_len];
            let read = reader.read_at(0, &mut output);
            drop(read);
        }
        let observed_pending = {
            let mut output = vec![0; probe_len];
            let mut read = pin!(reader.read_at(0, &mut output));
            matches!(read.as_mut().poll(&mut context), Poll::Pending)
        };
        let disarmed = probe_guard.disarm();
        drop(probe_guard);
        if !disarmed {
            return Err(ContractFailure::message_only("pending random-access probe could not be disarmed").at(id));
        }
        if !observed_pending {
            return Ok(ContractCheckOutcome::Unverified {
                reason: "injected read did not return Pending when first polled".to_owned(),
            });
        }
        let range_limit = self.context.properties().limits().max_read_range_bytes();
        verify_sample(&mut reader, 2, b"cd", range_limit, id).await?;
        Ok(ContractCheckOutcome::Passed)
    }
}

/// Reads the requested random-access range and compares it with the expected
/// bytes.
///
/// # Parameters
///
/// * `reader` — the open random-access reader used to retrieve the sample.
/// * `offset` — the byte position from which the random-access sample is read.
/// * `expected` — the bytes or value required by the selected contract check.
/// * `range_limit` — the maximum byte count permitted for one read request.
/// * `id` — the stable identity of the contract check being executed or
///   recorded.
async fn verify_sample(
    reader: &mut qubit_fs::read::AsyncRandomAccessReader,
    offset: u64,
    expected: &[u8],
    range_limit: FileSystemLimit,
    id: ContractCheckId,
) -> Result<(), ContractFailure> {
    let mut actual = vec![0; expected.len()];
    let maximum = range_limit
        .maximum()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(usize::MAX);
    let mut filled = 0;
    while filled < actual.len() {
        let request_len = (actual.len() - filled).min(maximum);
        if request_len == 0 {
            return Err(ContractFailure::message_only(
                "random-access range limit prevents a non-empty sample read",
            )
            .at(id));
        }
        let count = reader
            .read_at(offset + filled as u64, &mut actual[filled..filled + request_len])
            .await
            .map_err(|error| ContractFailure::with_source("random-access sample read failed", error).at(id))?;
        if count == 0 {
            break;
        }
        filled += count;
    }
    if actual[..filled] != *expected {
        return Err(ContractFailure::message_only(format!(
            "random-access sample mismatch at offset {offset}: read {filled} of {} bytes",
            expected.len()
        ))
        .at(id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::task::Context;
    use std::task::Poll;
    use std::task::Waker;

    use qubit_fs::AsyncFileSystem;
    use qubit_fs::FsResult;
    use qubit_fs::error::FsError;
    use qubit_fs::error::FsErrorKind;
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
    use qubit_fs::spi::AsyncFileSystemSpi;
    use qubit_fs::spi::AsyncRandomAccessReadSession;
    use qubit_fs::spi::OpenReaderRequest;
    use qubit_fs::spi::OpenedAsyncRandomAccessReader;
    use qubit_fs::spi::ProviderOperation;
    use qubit_fs::spi::ProviderOperations;
    use qubit_fs::spi::ProviderProperties;
    use qubit_fs::spi::SpiFuture;
    use qubit_fs::spi::StatRequest;
    use qubit_fs::spi::StatResponse;

    use crate::AsyncFileSystemContractSuite;
    use crate::AsyncFileSystemFixture;
    use crate::ContractCheckId;
    use crate::ContractCheckOutcome;
    use crate::FixtureError;
    use crate::FixtureFuture;
    use crate::FixtureResult;
    use crate::FixtureSupport;

    const PROVIDER_ID: &str = "async-random-access-unit-fixture";

    struct RandomAccessProvider {
        current: Arc<Mutex<Arc<[u8]>>>,
        supports_random_access: bool,
        reported_len: Option<u64>,
        reopen_path: bool,
        open_fails: bool,
        range_limit: FileSystemLimit,
    }

    struct RandomAccessSession {
        bytes: Arc<[u8]>,
        current: Arc<Mutex<Arc<[u8]>>>,
        reopen_path: bool,
    }

    impl AsyncRandomAccessReadSession for RandomAccessSession {
        fn read_at_async<'a>(
            self: Pin<&'a mut Self>,
            offset: u64,
            output: &'a mut [u8],
        ) -> SpiFuture<'a, FsResult<usize>> {
            let session = self.get_mut();
            let bytes = session.bytes.clone();
            let current = session.current.clone();
            let reopen_path = session.reopen_path;
            Box::pin(async move {
                let bytes = if reopen_path {
                    current.lock().expect("fixture state").clone()
                } else {
                    bytes
                };
                let start = usize::try_from(offset).unwrap_or(usize::MAX);
                let count = output.len().min(1).min(bytes.len().saturating_sub(start));
                output[..count].copy_from_slice(&bytes[start..start + count]);
                Ok(count)
            })
        }
    }

    impl AsyncFileSystemSpi for RandomAccessProvider {
        fn properties(&self) -> ProviderProperties {
            let mut operations = ProviderOperations::new().with(ProviderOperation::OpenReader);
            if self.supports_random_access {
                operations = operations.with(ProviderOperation::OpenRandomAccessReader);
            }
            ProviderProperties::new(
                FileSystemInfo::new(
                    FileSystemId::new(PROVIDER_ID).expect("valid fixture ID"),
                    "fixture",
                    PathSemantics::Hierarchical,
                ),
                operations,
                FileSystemCapabilities::new().with_guaranteed(FileSystemCapability::Read),
                FileSystemLimits::unknown().with_max_read_range_bytes(self.range_limit),
                PathConstraints::absolute(),
                SymlinkPolicy::Reject,
            )
            .expect("valid fixture properties")
        }

        fn stat<'a>(&'a self, request: StatRequest<'a>) -> SpiFuture<'a, FsResult<StatResponse>> {
            let len = self.current.lock().expect("fixture state").len() as u64;
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
            let bytes = self.current.lock().expect("fixture state").clone();
            let reported_len = self.reported_len.unwrap_or(bytes.len() as u64);
            let supports_random_access = self.supports_random_access;
            let open_fails = self.open_fails;
            let current = self.current.clone();
            let reopen_path = self.reopen_path;
            Box::pin(async move {
                if open_fails {
                    return Err(FsError::new(
                        FsErrorKind::Io,
                        qubit_fs::error::FsOperation::OpenReader,
                        "injected random-access open failure",
                    ));
                }
                if !supports_random_access {
                    return Err(FsError::new(
                        qubit_fs::error::FsErrorKind::UnsupportedOperation,
                        qubit_fs::error::FsOperation::OpenReader,
                        "random access unsupported",
                    ));
                }
                let info = OpenedFileInfo::new(
                    FileSystemId::new(PROVIDER_ID).expect("valid fixture ID"),
                    request.path().clone(),
                )
                .with_metadata(FileMetadata::new(FileKind::File).with_len(Some(reported_len)));
                Ok(OpenedAsyncRandomAccessReader::new(
                    info,
                    Box::new(RandomAccessSession {
                        bytes,
                        current,
                        reopen_path,
                    }),
                ))
            })
        }
    }

    struct RandomAccessFixture {
        file_system: AsyncFileSystem,
        current: Arc<Mutex<Arc<[u8]>>>,
    }

    impl RandomAccessFixture {
        fn new() -> Self {
            Self::with_options(true, None, false)
        }

        fn with_options(supports_random_access: bool, reported_len: Option<u64>, reopen_path: bool) -> Self {
            Self::with_configuration(
                supports_random_access,
                reported_len,
                reopen_path,
                false,
                FileSystemLimit::Unknown,
            )
        }

        fn with_configuration(
            supports_random_access: bool,
            reported_len: Option<u64>,
            reopen_path: bool,
            open_fails: bool,
            range_limit: FileSystemLimit,
        ) -> Self {
            let current = Arc::new(Mutex::new(Arc::from(&b""[..])));
            let file_system = AsyncFileSystem::from_spi(RandomAccessProvider {
                current: current.clone(),
                supports_random_access,
                reported_len,
                reopen_path,
                open_fails,
                range_limit,
            })
            .expect("fixture provider initializes");
            Self { file_system, current }
        }
    }

    impl AsyncFileSystemFixture for RandomAccessFixture {
        fn file_system(&self) -> &AsyncFileSystem {
            &self.file_system
        }

        fn path(&self, relative: &str) -> FixtureResult<Path> {
            Path::parse(&format!("/{relative}"))
                .map_err(|error| FixtureError::with_source("invalid fixture path", error))
        }

        fn teardown(&self) -> FixtureFuture<'_, ()> {
            *self.current.lock().expect("fixture state") = Arc::from(&b""[..]);
            Box::pin(async { Ok(()) })
        }

        fn random_access_operation(&self) -> FixtureSupport<bool> {
            FixtureSupport::Supported(true)
        }

        fn seed_file<'a>(&'a self, relative: &'a str, bytes: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<Path>> {
            let path = self.path(relative);
            Box::pin(async move {
                let path = path?;
                *self.current.lock().expect("fixture state") = Arc::from(bytes);
                Ok(FixtureSupport::Supported(path))
            })
        }

        fn replace_random_access_path<'a>(
            &'a self,
            _path: &'a Path,
            bytes: &'a [u8],
        ) -> FixtureFuture<'a, FixtureSupport<()>> {
            *self.current.lock().expect("fixture state") = Arc::from(bytes);
            Box::pin(async { Ok(FixtureSupport::Supported(())) })
        }
    }

    fn ready<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("random-access fixture operations complete immediately"),
        }
    }

    #[test]
    fn test_basic_and_identity_checks_verify_partial_reads_and_object_snapshots() {
        let fixture = RandomAccessFixture::new();
        let mut basic_suite = AsyncFileSystemContractSuite::new(&fixture);
        let basic = ready(basic_suite.run_check(ContractCheckId::RandomAccessBasic));
        assert!(basic.failures().is_empty(), "{:?}", basic.failures());
        assert_eq!(basic.report().checks()[0].outcome(), &ContractCheckOutcome::Passed);

        let mut identity_suite = AsyncFileSystemContractSuite::new(&fixture);
        let identity = ready(identity_suite.run_check(ContractCheckId::RandomAccessIdentity));
        assert!(identity.failures().is_empty(), "{:?}", identity.failures());
        assert_eq!(identity.report().checks()[0].outcome(), &ContractCheckOutcome::Passed);
    }

    #[test]
    fn test_undeclared_random_access_is_rejected_when_provider_accepts_it() {
        struct UnsupportedDeclaration(RandomAccessFixture);

        impl AsyncFileSystemFixture for UnsupportedDeclaration {
            fn file_system(&self) -> &AsyncFileSystem {
                self.0.file_system()
            }

            fn path(&self, relative: &str) -> FixtureResult<Path> {
                self.0.path(relative)
            }

            fn teardown(&self) -> FixtureFuture<'_, ()> {
                self.0.teardown()
            }

            fn random_access_operation(&self) -> FixtureSupport<bool> {
                FixtureSupport::Supported(false)
            }
        }

        let fixture = UnsupportedDeclaration(RandomAccessFixture::new());
        *fixture.0.current.lock().expect("fixture state") = Arc::from(&b"abcdef"[..]);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(ContractCheckId::RandomAccessBasic));
        assert_eq!(run.failures().len(), 1);
        assert!(
            run.failures()[0]
                .message()
                .contains("declaring the operation unsupported")
        );
        assert_eq!(
            run.report().checks()[0].outcome(),
            &ContractCheckOutcome::Failed {
                reason: "provider opened a random-access reader after declaring the operation unsupported".to_owned(),
            }
        );
    }

    #[test]
    fn test_random_access_is_skipped_or_rejected_when_fixture_does_not_declare_support() {
        struct NoDeclaration(RandomAccessFixture);

        impl AsyncFileSystemFixture for NoDeclaration {
            fn file_system(&self) -> &AsyncFileSystem {
                self.0.file_system()
            }
            fn path(&self, relative: &str) -> FixtureResult<Path> {
                self.0.path(relative)
            }
            fn teardown(&self) -> FixtureFuture<'_, ()> {
                self.0.teardown()
            }
        }

        let fixture = NoDeclaration(RandomAccessFixture::new());
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(ContractCheckId::RandomAccessBasic));
        assert_eq!(
            run.report().checks()[0].outcome(),
            &ContractCheckOutcome::SkippedOptional {
                reason: "fixture has no independent random-access operation declaration".to_owned(),
            }
        );

        struct FalseDeclaration(RandomAccessFixture);

        impl AsyncFileSystemFixture for FalseDeclaration {
            fn file_system(&self) -> &AsyncFileSystem {
                self.0.file_system()
            }
            fn path(&self, relative: &str) -> FixtureResult<Path> {
                self.0.path(relative)
            }
            fn teardown(&self) -> FixtureFuture<'_, ()> {
                self.0.teardown()
            }
            fn random_access_operation(&self) -> FixtureSupport<bool> {
                FixtureSupport::Supported(false)
            }
        }

        let fixture = FalseDeclaration(RandomAccessFixture::with_options(false, None, false));
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(ContractCheckId::RandomAccessBasic));
        assert!(run.failures().is_empty(), "{:?}", run.failures());
        assert_eq!(
            run.report().checks()[0].outcome(),
            &ContractCheckOutcome::RejectedAsExpected
        );
    }

    #[test]
    fn test_random_access_seed_and_open_metadata_failures_are_reported() {
        struct NoSeed(RandomAccessFixture);

        impl AsyncFileSystemFixture for NoSeed {
            fn file_system(&self) -> &AsyncFileSystem {
                self.0.file_system()
            }
            fn path(&self, relative: &str) -> FixtureResult<Path> {
                self.0.path(relative)
            }
            fn teardown(&self) -> FixtureFuture<'_, ()> {
                self.0.teardown()
            }
            fn random_access_operation(&self) -> FixtureSupport<bool> {
                FixtureSupport::Supported(true)
            }
            fn seed_file<'a>(
                &'a self,
                _relative: &'a str,
                _bytes: &'a [u8],
            ) -> FixtureFuture<'a, FixtureSupport<Path>> {
                Box::pin(async { Ok(FixtureSupport::Unsupported) })
            }
        }

        let fixture = NoSeed(RandomAccessFixture::new());
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(ContractCheckId::RandomAccessBasic));
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { .. }
        ));

        let fixture = RandomAccessFixture::with_options(true, Some(7), false);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(ContractCheckId::RandomAccessBasic));
        assert_eq!(run.failures().len(), 1);
        assert!(run.failures()[0].message().contains("open-time length"));

        let fixture = RandomAccessFixture::with_options(true, None, true);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(ContractCheckId::RandomAccessIdentity));
        assert_eq!(run.failures().len(), 1);
        assert!(run.failures()[0].message().contains("sample mismatch"));
    }

    #[test]
    fn test_random_access_cancellation_is_skipped_without_pending_probe_support() {
        let fixture = RandomAccessFixture::new();
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(ContractCheckId::RandomAccessCancel));
        assert!(run.failures().is_empty(), "{:?}", run.failures());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::SkippedOptional { .. }
        ));
    }

    #[test]
    fn test_random_access_open_failures_are_reported_for_declared_and_undeclared_operations() {
        let fixture = RandomAccessFixture::with_configuration(true, None, false, true, FileSystemLimit::Unknown);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(ContractCheckId::RandomAccessBasic));
        assert_eq!(run.failures().len(), 1);
        assert!(
            run.failures()[0]
                .message()
                .contains("declared random-access open failed")
        );

        let fixture = RandomAccessFixture::with_configuration(true, None, false, true, FileSystemLimit::Unknown);
        struct Undeclared(RandomAccessFixture);
        impl AsyncFileSystemFixture for Undeclared {
            fn file_system(&self) -> &AsyncFileSystem {
                self.0.file_system()
            }
            fn path(&self, relative: &str) -> FixtureResult<Path> {
                self.0.path(relative)
            }
            fn teardown(&self) -> FixtureFuture<'_, ()> {
                self.0.teardown()
            }
            fn random_access_operation(&self) -> FixtureSupport<bool> {
                FixtureSupport::Supported(false)
            }
        }
        let fixture = Undeclared(fixture);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(ContractCheckId::RandomAccessBasic));
        assert_eq!(run.failures().len(), 1);
        assert!(
            run.failures()[0]
                .message()
                .contains("undeclared random-access operation returned an unexpected error")
        );
    }

    #[test]
    fn test_random_access_finite_range_limits_are_probed_or_skipped_safely() {
        let fixture = RandomAccessFixture::with_configuration(true, None, false, false, FileSystemLimit::Maximum(3));
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(ContractCheckId::RandomAccessBounds));
        assert!(run.failures().is_empty(), "{:?}", run.failures());
        assert_eq!(run.report().checks()[0].outcome(), &ContractCheckOutcome::Passed);

        let fixture = RandomAccessFixture::with_configuration(true, None, false, false, FileSystemLimit::Maximum(64));
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(ContractCheckId::RandomAccessBounds));
        assert_eq!(
            run.report().checks()[0].outcome(),
            &ContractCheckOutcome::SkippedOptional {
                reason: "finite random-access range limit exceeds the 64-byte probe budget".to_owned(),
            }
        );

        let fixture = RandomAccessFixture::with_configuration(true, None, false, false, FileSystemLimit::Maximum(6));
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(ContractCheckId::RandomAccessBounds));
        assert_eq!(
            run.report().checks()[0].outcome(),
            &ContractCheckOutcome::SkippedOptional {
                reason: "seed is too short to probe the declared random-access range limit".to_owned(),
            }
        );
    }

    #[test]
    fn test_random_access_identity_check_is_skipped_without_replacement_hook() {
        struct NoReplacement(RandomAccessFixture);
        impl AsyncFileSystemFixture for NoReplacement {
            fn file_system(&self) -> &AsyncFileSystem {
                self.0.file_system()
            }
            fn path(&self, relative: &str) -> FixtureResult<Path> {
                self.0.path(relative)
            }
            fn teardown(&self) -> FixtureFuture<'_, ()> {
                self.0.teardown()
            }
        }
        let fixture = NoReplacement(RandomAccessFixture::new());
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = ready(suite.run_check(ContractCheckId::RandomAccessIdentity));
        assert!(run.failures().is_empty(), "{:?}", run.failures());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::SkippedOptional { .. }
        ));
    }
}
