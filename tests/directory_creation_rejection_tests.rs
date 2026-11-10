// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Existing directories must be rejected unless `exists_ok` is enabled.

mod common;

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

pub use ::qubit_fs_testkit;
use qubit_fs::FileSystem;
use qubit_fs::error::FsError;
use qubit_fs::error::FsErrorKind;
use qubit_fs::path::Path;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContractSuite;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;

use self::common::MemoryFault;
use self::common::MemoryFixture;

const DIRECTORY_CHECKS: [ContractCheckId; 2] = [ContractCheckId::DirectoryCreate, ContractCheckId::DirectoryRecursive];

/// Conforming synchronous directory operations accept and reject according
/// to the `exists_ok` option, while cleaning every created directory.
#[test]
fn test_sync_directory_creation_rejection_contract() {
    for id in DIRECTORY_CHECKS {
        let fixture = MemoryFixture::new();
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        run.assert_satisfied();
        assert!(run.cleanup().completed(), "{id}");
        assert!(fixture.is_empty(), "{id}: fixture cleanup must remove directories");
    }
}

/// The contract rejects a facade contract violation, wrong error kind, and a
/// nominal rejection that removes the existing target.
#[test]
fn test_sync_directory_creation_rejection_detects_provider_faults() {
    for fault in [
        MemoryFault::IgnoreDirectoryExistsOk,
        MemoryFault::DirectoryRejectPermissionDenied,
        MemoryFault::DirectoryRejectWrongProvider,
        MemoryFault::DirectoryRejectRemovesTarget,
    ] {
        for id in DIRECTORY_CHECKS {
            let fixture = MemoryFixture::with_fault(fault);
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id);
            assert!(!run.requirements_satisfied(), "{fault:?} unexpectedly passed {id}");
            assert_eq!(run.failures().len(), 1, "{fault:?} at {id}: {:?}", run.failures());
            assert_eq!(run.failures()[0].check(), Some(id), "{fault:?}");
            if fault == MemoryFault::IgnoreDirectoryExistsOk {
                assert_eq!(
                    std::error::Error::source(&run.failures()[0])
                        .and_then(|error| error.downcast_ref::<FsError>())
                        .map(FsError::kind),
                    Some(FsErrorKind::ProviderContractViolation),
                    "the facade's provider contract violation must not count as AlreadyExists",
                );
            }
            assert!(run.cleanup().completed(), "{fault:?} at {id}");
            assert!(
                fixture.is_empty(),
                "{fault:?} at {id}: fixture cleanup must remove directories"
            );
        }
    }
}

#[derive(Clone, Copy)]
enum ObservationMode {
    Unsupported,
    Error,
}

struct SyncObservationFixture {
    inner: MemoryFixture,
    calls: AtomicUsize,
    mode: ObservationMode,
}

impl FileSystemFixture for SyncObservationFixture {
    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn exists_out_of_band(&self, path: &Path) -> FixtureResult<FixtureSupport<bool>> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call < 4 || (matches!(self.mode, ObservationMode::Error) && call < 6) {
            return self.inner.exists_out_of_band(path);
        }
        match self.mode {
            ObservationMode::Unsupported => Ok(FixtureSupport::Unsupported),
            ObservationMode::Error => Err(FixtureError::new("injected rejection observation failure")),
        }
    }

    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }
}

/// Missing independent rejection evidence remains unverified, and observation
/// errors retain their source instead of being treated as a successful reject.
#[test]
fn test_sync_directory_rejection_requires_independent_observation() {
    for mode in [ObservationMode::Unsupported, ObservationMode::Error] {
        let fixture = SyncObservationFixture {
            inner: MemoryFixture::new(),
            calls: AtomicUsize::new(0),
            mode,
        };
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::DirectoryCreate);
        assert!(!run.requirements_satisfied());
        match mode {
            ObservationMode::Unsupported => {
                assert!(run.failures().is_empty());
                assert_eq!(
                    fixture.calls.load(Ordering::SeqCst),
                    8,
                    "both paths must be observed before and after rejection",
                );
                assert!(matches!(
                    run.report().checks()[0].outcome(),
                    ContractCheckOutcome::Unverified { .. }
                ));
            }
            ObservationMode::Error => {
                assert_eq!(run.failures().len(), 1);
                assert_eq!(run.failures()[0].check(), Some(ContractCheckId::DirectoryCreate));
                assert!(std::error::Error::source(&run.failures()[0]).is_some());
            }
        }
        assert!(run.cleanup().completed());
        assert!(fixture.inner.is_empty());
    }
}

#[cfg(feature = "async")]
mod asynchronous {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use qubit_fs::AsyncFileSystem;
    use qubit_fs::error::FsError;
    use qubit_fs::error::FsErrorKind;
    use qubit_fs::path::Path;
    use qubit_fs_testkit::AsyncFileSystemContractSuite;
    use qubit_fs_testkit::AsyncFileSystemFixture;
    use qubit_fs_testkit::ContractCheckId;
    use qubit_fs_testkit::ContractCheckOutcome;
    use qubit_fs_testkit::FixtureError;
    use qubit_fs_testkit::FixtureFuture;
    use qubit_fs_testkit::FixtureResult;
    use qubit_fs_testkit::FixtureSupport;

    use super::DIRECTORY_CHECKS;
    use super::ObservationMode;
    use super::common::AsyncMemoryFault;
    use super::common::AsyncMemoryFixture;
    use super::common::async_memory_file_system::run_controlled;

    struct AsyncObservationFixture {
        inner: AsyncMemoryFixture,
        calls: AtomicUsize,
        mode: ObservationMode,
    }

    impl AsyncFileSystemFixture for AsyncObservationFixture {
        fn file_system(&self) -> &AsyncFileSystem {
            self.inner.file_system()
        }

        fn path(&self, relative: &str) -> FixtureResult<Path> {
            self.inner.path(relative)
        }

        fn exists_out_of_band<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<bool>> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call < 4 || (matches!(self.mode, ObservationMode::Error) && call < 6) {
                return self.inner.exists_out_of_band(path);
            }
            match self.mode {
                ObservationMode::Unsupported => Box::pin(async { Ok(FixtureSupport::Unsupported) }),
                ObservationMode::Error => {
                    Box::pin(async { Err(FixtureError::new("injected rejection observation failure")) })
                }
            }
        }

        fn teardown(&self) -> FixtureFuture<'_, ()> {
            self.inner.teardown()
        }
    }

    /// Async directory operations enforce the same rejection and cleanup
    /// contract as their synchronous counterparts.
    #[test]
    fn test_async_directory_creation_rejection_contract() {
        run_controlled(async {
            for id in DIRECTORY_CHECKS {
                let fixture = AsyncMemoryFixture::new();
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                run.assert_satisfied();
                assert!(run.cleanup().completed(), "{id}");
                assert!(fixture.is_empty(), "{id}: fixture cleanup must remove directories");
            }
        });
    }

    /// Async rejection detects the same independently injected provider
    /// defects as the synchronous suite.
    #[test]
    fn test_async_directory_creation_rejection_detects_provider_faults() {
        run_controlled(async {
            for fault in [
                AsyncMemoryFault::IgnoreDirectoryExistsOk,
                AsyncMemoryFault::DirectoryRejectPermissionDenied,
                AsyncMemoryFault::DirectoryRejectWrongProvider,
                AsyncMemoryFault::DirectoryRejectRemovesTarget,
            ] {
                for id in DIRECTORY_CHECKS {
                    let fixture = AsyncMemoryFixture::with_fault(fault);
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert!(!run.requirements_satisfied(), "{fault:?} unexpectedly passed {id}");
                    assert_eq!(run.failures().len(), 1, "{fault:?} at {id}: {:?}", run.failures());
                    assert_eq!(run.failures()[0].check(), Some(id), "{fault:?}");
                    if fault == AsyncMemoryFault::IgnoreDirectoryExistsOk {
                        assert_eq!(
                            std::error::Error::source(&run.failures()[0])
                                .and_then(|error| error.downcast_ref::<FsError>())
                                .map(FsError::kind),
                            Some(FsErrorKind::ProviderContractViolation),
                            "the facade's provider contract violation must not count as AlreadyExists",
                        );
                    }
                    assert!(run.cleanup().completed(), "{fault:?} at {id}");
                    assert!(
                        fixture.is_empty(),
                        "{fault:?} at {id}: fixture cleanup must remove directories"
                    );
                }
            }
        });
    }

    /// Async rejection must retain independent observation as an essential
    /// premise and preserve failures from that observation.
    #[test]
    fn test_async_directory_rejection_requires_independent_observation() {
        run_controlled(async {
            for mode in [ObservationMode::Unsupported, ObservationMode::Error] {
                let fixture = AsyncObservationFixture {
                    inner: AsyncMemoryFixture::new(),
                    calls: AtomicUsize::new(0),
                    mode,
                };
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(ContractCheckId::DirectoryCreate).await;
                assert!(!run.requirements_satisfied());
                match mode {
                    ObservationMode::Unsupported => {
                        assert!(run.failures().is_empty());
                        assert_eq!(
                            fixture.calls.load(Ordering::SeqCst),
                            8,
                            "both paths must be observed before and after rejection",
                        );
                        assert!(matches!(
                            run.report().checks()[0].outcome(),
                            ContractCheckOutcome::Unverified { .. }
                        ));
                    }
                    ObservationMode::Error => {
                        assert_eq!(run.failures().len(), 1);
                        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::DirectoryCreate));
                        assert!(std::error::Error::source(&run.failures()[0]).is_some());
                    }
                }
                assert!(run.cleanup().completed());
                assert!(fixture.inner.is_empty());
            }
        });
    }
}
