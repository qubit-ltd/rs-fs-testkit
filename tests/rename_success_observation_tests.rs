// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Independent observations for successful and conflicting rename checks.

pub use ::qubit_fs_testkit;
mod common;

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use common::MemoryFixture;
use qubit_fs::FileSystem;
use qubit_fs::path::Path;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContractSuite;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;

struct SyncFixture {
    inner: MemoryFixture,
    unsupported_read: Option<usize>,
    unsupported_exists: Option<usize>,
    failed_read: Option<usize>,
    wrong_read: Option<usize>,
    wrong_exists: Option<usize>,
    reads: AtomicUsize,
    exists: AtomicUsize,
}

impl SyncFixture {
    fn new(unsupported_read: Option<usize>, unsupported_exists: Option<usize>) -> Self {
        Self {
            inner: MemoryFixture::with_all_capabilities(),
            unsupported_read,
            unsupported_exists,
            failed_read: None,
            wrong_read: None,
            wrong_exists: None,
            reads: AtomicUsize::new(0),
            exists: AtomicUsize::new(0),
        }
    }
}

impl FileSystemFixture for SyncFixture {
    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }
    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }
    fn seed_file(&self, relative: &str, bytes: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        self.inner.seed_file(relative, bytes)
    }
    fn read_file(&self, path: &Path) -> FixtureResult<FixtureSupport<Vec<u8>>> {
        let call = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
        if self.failed_read == Some(call) {
            return Err(FixtureError::new("injected read observation failure"));
        }
        if self.wrong_read == Some(call) {
            return Ok(FixtureSupport::Supported(b"wrong snapshot".to_vec()));
        }
        if self.unsupported_read == Some(call) {
            Ok(FixtureSupport::Unsupported)
        } else {
            self.inner.read_file(path)
        }
    }
    fn exists_out_of_band(&self, path: &Path) -> FixtureResult<FixtureSupport<bool>> {
        let call = self.exists.fetch_add(1, Ordering::SeqCst) + 1;
        if self.wrong_exists == Some(call) {
            return Ok(FixtureSupport::Supported(true));
        }
        if self.unsupported_exists == Some(call) {
            Ok(FixtureSupport::Unsupported)
        } else {
            self.inner.exists_out_of_band(path)
        }
    }
}

fn assert_unverified(fixture: &SyncFixture, id: ContractCheckId, calls: usize, phase: &str) {
    let mut suite = FileSystemContractSuite::new(fixture);
    let run = suite.run_check(id);
    assert!(!run.requirements_satisfied(), "{id}");
    let ContractCheckOutcome::Unverified { reason } = run.report().checks()[0].outcome() else {
        panic!("{id}: {:?}", run.report().checks());
    };
    assert!(
        reason.contains("rename")
            && (reason.contains("read_file") || reason.contains("exists_out_of_band"))
            && reason.contains(phase),
        "{reason}"
    );
    assert!(!run.failures().iter().any(|failure| failure.check() == Some(id)));
    assert!(run.cleanup().completed());
    assert!(fixture.inner.is_empty());
    assert_eq!(
        fixture.inner.rename_spi_call_count(),
        calls,
        "must stop before the next rename"
    );
}

#[test]
fn test_sync_rename_missing_observation_evidence_is_unverified_and_stops() {
    for id in [
        ContractCheckId::RenameBasic,
        ContractCheckId::RenameAtomic,
        ContractCheckId::RenameDurable,
    ] {
        for (read, exists, expected_calls, phase) in [
            (Some(1), None, 0, "initial source contents"),
            (None, Some(1), 0, "initial target absence"),
            (Some(2), None, 1, "target contents after rename"),
            (None, Some(2), 1, "source removal after rename"),
        ] {
            let fixture = SyncFixture::new(read, exists);
            assert_unverified(&fixture, id, expected_calls, phase);
        }
    }
    for (read, expected_calls, phase) in [
        (Some(1), 0, "initial source contents"),
        (Some(2), 0, "initial conflict target contents"),
        (Some(3), 1, "source contents after conflict rejection"),
        (Some(4), 1, "target contents after conflict rejection"),
    ] {
        let fixture = SyncFixture::new(read, None);
        assert_unverified(&fixture, ContractCheckId::RenameConflict, expected_calls, phase);
    }
}

#[test]
fn test_sync_rename_supported_observation_defects_remain_failures() {
    for fixture in [
        SyncFixture {
            wrong_read: Some(2),
            ..SyncFixture::new(None, None)
        },
        SyncFixture {
            wrong_exists: Some(2),
            ..SyncFixture::new(None, None)
        },
        SyncFixture {
            failed_read: Some(2),
            ..SyncFixture::new(None, None)
        },
    ] {
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::RenameBasic);
        assert!(!run.requirements_satisfied());
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::RenameBasic));
        assert!(run.cleanup().completed());
        assert!(fixture.inner.is_empty());
        if fixture.failed_read.is_some() {
            assert!(std::error::Error::source(&run.failures()[0]).is_some());
        }
    }
}

#[test]
fn test_sync_rename_normal_fixture_still_passes() {
    for id in [
        ContractCheckId::RenameBasic,
        ContractCheckId::RenameConflict,
        ContractCheckId::RenameAtomic,
        ContractCheckId::RenameDurable,
    ] {
        let fixture = SyncFixture::new(None, None);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        assert!(run.requirements_satisfied(), "{id}: {:?}", run.failures());
        assert!(run.cleanup().completed());
        assert!(fixture.inner.is_empty());
    }
}

#[cfg(feature = "async")]
mod asynchronous {
    use qubit_fs::AsyncFileSystem;
    use qubit_fs_testkit::AsyncFileSystemContractSuite;
    use qubit_fs_testkit::AsyncFileSystemFixture;
    use qubit_fs_testkit::FixtureFuture;

    use super::AtomicUsize;
    use super::ContractCheckId;
    use super::ContractCheckOutcome;
    use super::FixtureError;
    use super::FixtureResult;
    use super::FixtureSupport;
    use super::Ordering;
    use super::Path;
    use crate::common::AsyncMemoryFixture;
    use crate::common::async_memory_file_system::run_controlled;

    struct AsyncFixture {
        inner: AsyncMemoryFixture,
        unsupported_read: Option<usize>,
        unsupported_exists: Option<usize>,
        failed_read: Option<usize>,
        wrong_read: Option<usize>,
        wrong_exists: Option<usize>,
        reads: AtomicUsize,
        exists: AtomicUsize,
    }

    impl AsyncFixture {
        fn new(unsupported_read: Option<usize>, unsupported_exists: Option<usize>) -> Self {
            Self {
                inner: AsyncMemoryFixture::new(),
                unsupported_read,
                unsupported_exists,
                failed_read: None,
                wrong_read: None,
                wrong_exists: None,
                reads: AtomicUsize::new(0),
                exists: AtomicUsize::new(0),
            }
        }
    }

    impl AsyncFileSystemFixture for AsyncFixture {
        fn file_system(&self) -> &AsyncFileSystem {
            self.inner.file_system()
        }
        fn path(&self, relative: &str) -> FixtureResult<Path> {
            self.inner.path(relative)
        }
        fn seed_file<'a>(&'a self, relative: &'a str, bytes: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<Path>> {
            self.inner.seed_file(relative, bytes)
        }
        fn read_file<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<Vec<u8>>> {
            let call = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
            if self.failed_read == Some(call) {
                return Box::pin(async { Err(FixtureError::new("injected read observation failure")) });
            }
            if self.wrong_read == Some(call) {
                return Box::pin(async { Ok(FixtureSupport::Supported(b"wrong snapshot".to_vec())) });
            }
            if self.unsupported_read == Some(call) {
                Box::pin(async { Ok(FixtureSupport::Unsupported) })
            } else {
                self.inner.read_file(path)
            }
        }
        fn exists_out_of_band<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<bool>> {
            let call = self.exists.fetch_add(1, Ordering::SeqCst) + 1;
            if self.wrong_exists == Some(call) {
                return Box::pin(async { Ok(FixtureSupport::Supported(true)) });
            }
            if self.unsupported_exists == Some(call) {
                Box::pin(async { Ok(FixtureSupport::Unsupported) })
            } else {
                self.inner.exists_out_of_band(path)
            }
        }
        fn teardown(&self) -> FixtureFuture<'_, ()> {
            self.inner.teardown()
        }
    }

    #[test]
    fn test_async_rename_missing_observation_evidence_is_unverified_and_stops() {
        run_controlled(async {
            for id in [
                ContractCheckId::RenameBasic,
                ContractCheckId::RenameAtomic,
                ContractCheckId::RenameDurable,
            ] {
                for (read, exists, expected_calls, phase) in [
                    (Some(1), None, 0, "initial source contents"),
                    (None, Some(1), 0, "initial target absence"),
                    (Some(2), None, 1, "target contents after rename"),
                    (None, Some(2), 1, "source removal after rename"),
                ] {
                    let fixture = AsyncFixture::new(read, exists);
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert!(!run.requirements_satisfied());
                    let ContractCheckOutcome::Unverified { reason } = run.report().checks()[0].outcome() else {
                        panic!("{id}: {:?}", run.report().checks());
                    };
                    assert!(
                        reason.contains("rename")
                            && (reason.contains("read_file") || reason.contains("exists_out_of_band"))
                            && reason.contains(phase),
                        "{reason}"
                    );
                    assert!(!run.failures().iter().any(|failure| failure.check() == Some(id)));
                    assert!(run.cleanup().completed());
                    assert!(fixture.inner.is_empty());
                    assert_eq!(fixture.inner.rename_spi_call_count(), expected_calls);
                }
            }
            for (read, expected_calls, phase) in [
                (Some(1), 0, "initial source contents"),
                (Some(2), 0, "initial conflict target contents"),
                (Some(3), 1, "source contents after conflict rejection"),
                (Some(4), 1, "target contents after conflict rejection"),
            ] {
                let fixture = AsyncFixture::new(read, None);
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(ContractCheckId::RenameConflict).await;
                assert!(!run.requirements_satisfied());
                assert!(matches!(
                    run.report().checks()[0].outcome(),
                    ContractCheckOutcome::Unverified { .. }
                ));
                let ContractCheckOutcome::Unverified { reason } = run.report().checks()[0].outcome() else {
                    unreachable!();
                };
                assert!(reason.contains(phase), "{reason}");
                assert!(run.cleanup().completed());
                assert!(fixture.inner.is_empty());
                assert_eq!(fixture.inner.rename_spi_call_count(), expected_calls);
            }
        });
    }

    #[test]
    fn test_async_rename_supported_observation_defects_remain_failures() {
        run_controlled(async {
            for fixture in [
                AsyncFixture {
                    wrong_read: Some(2),
                    ..AsyncFixture::new(None, None)
                },
                AsyncFixture {
                    wrong_exists: Some(2),
                    ..AsyncFixture::new(None, None)
                },
                AsyncFixture {
                    failed_read: Some(2),
                    ..AsyncFixture::new(None, None)
                },
            ] {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(ContractCheckId::RenameBasic).await;
                assert!(!run.requirements_satisfied());
                assert_eq!(run.failures()[0].check(), Some(ContractCheckId::RenameBasic));
                if fixture.failed_read.is_some() {
                    assert!(std::error::Error::source(&run.failures()[0]).is_some());
                }
                assert!(run.cleanup().completed());
                assert!(fixture.inner.is_empty());
            }
        });
    }

    #[test]
    fn test_async_rename_normal_fixture_still_passes() {
        run_controlled(async {
            for id in [
                ContractCheckId::RenameBasic,
                ContractCheckId::RenameConflict,
                ContractCheckId::RenameAtomic,
                ContractCheckId::RenameDurable,
            ] {
                let fixture = AsyncFixture::new(None, None);
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert!(run.requirements_satisfied(), "{id}: {:?}", run.failures());
                assert!(run.cleanup().completed());
                assert!(fixture.inner.is_empty());
            }
        });
    }
}
