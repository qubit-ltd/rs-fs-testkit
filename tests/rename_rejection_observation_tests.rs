// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0 (the "License");
//    you may not use this file except in compliance with the License.
// =============================================================================

mod common;

pub use ::qubit_fs_testkit;

use common::MemoryFixture;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::path::Path;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContractSuite;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

fn selected(outcome: &ContractCheckOutcome) -> bool {
    matches!(outcome, ContractCheckOutcome::RejectedAsExpected)
}

struct ObservationFixture {
    inner: MemoryFixture,
    no_seed: bool,
    unsupported_read_call: Option<usize>,
    failed_read_call: Option<usize>,
    changed_read_call: Option<usize>,
    unsupported_exists_call: Option<usize>,
    appeared_exists_call: Option<usize>,
    failed_exists_call: Option<usize>,
    read_calls: AtomicUsize,
    exists_calls: AtomicUsize,
}

impl ObservationFixture {
    fn new(
        no_seed: bool,
        unsupported_read_call: Option<usize>,
        failed_read_call: Option<usize>,
        changed_read_call: Option<usize>,
        unsupported_exists_call: Option<usize>,
        appeared_exists_call: Option<usize>,
        failed_exists_call: Option<usize>,
    ) -> Self {
        Self {
            inner: MemoryFixture::without_operation_capabilities(),
            no_seed,
            unsupported_read_call,
            failed_read_call,
            changed_read_call,
            unsupported_exists_call,
            appeared_exists_call,
            failed_exists_call,
            read_calls: AtomicUsize::new(0),
            exists_calls: AtomicUsize::new(0),
        }
    }
}

impl FileSystemFixture for ObservationFixture {
    fn file_system(&self) -> &qubit_fs::FileSystem {
        self.inner.file_system()
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }
    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }
    fn seed_file(&self, relative: &str, bytes: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        if self.no_seed {
            Ok(FixtureSupport::Unsupported)
        } else {
            self.inner.seed_file(relative, bytes)
        }
    }
    fn read_file(&self, path: &Path) -> FixtureResult<FixtureSupport<Vec<u8>>> {
        let call = self.read_calls.fetch_add(1, Ordering::Relaxed) + 1;
        if self.failed_read_call == Some(call) {
            return Err(FixtureError::new("injected read observation failure"));
        }
        if self.unsupported_read_call == Some(call) {
            return Ok(FixtureSupport::Unsupported);
        }
        if self.changed_read_call == Some(call) {
            return Ok(FixtureSupport::Supported(b"changed snapshot".to_vec()));
        }
        self.inner.read_file(path)
    }
    fn exists_out_of_band(&self, path: &Path) -> FixtureResult<FixtureSupport<bool>> {
        let call = self.exists_calls.fetch_add(1, Ordering::Relaxed) + 1;
        if self.failed_exists_call == Some(call) {
            return Err(FixtureError::new("injected existence observation failure"));
        }
        if self.appeared_exists_call == Some(call) {
            return Ok(FixtureSupport::Supported(true));
        }
        if self.unsupported_exists_call == Some(call) {
            return Ok(FixtureSupport::Unsupported);
        }
        self.inner.exists_out_of_band(path)
    }
}

#[test]
fn rename_rejections_have_independent_resource_observations() {
    for (fixture, id, capability) in [
        (
            MemoryFixture::without_operation_capabilities(),
            ContractCheckId::RenameBasic,
            FileSystemCapability::Rename,
        ),
        (
            MemoryFixture::without_optional_capabilities(),
            ContractCheckId::RenameAtomic,
            FileSystemCapability::AtomicRename,
        ),
        (
            MemoryFixture::without_optional_capabilities(),
            ContractCheckId::RenameDurable,
            FileSystemCapability::DurableRename,
        ),
    ] {
        assert!(
            !fixture
                .file_system()
                .properties()
                .capabilities()
                .supports(capability)
        );
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        run.assert_satisfied();
        assert!(selected(run.report().checks()[0].outcome()), "{id}");
        assert!(
            fixture.is_empty(),
            "prepared rejection seed must be cleaned up"
        );
        assert_eq!(
            fixture.rename_spi_call_count(),
            0,
            "preflight must reject before SPI"
        );
    }
}

#[test]
fn rename_rejection_without_baseline_observation_is_unverified() {
    let fixture = ObservationFixture::new(false, Some(1), None, None, None, None, None);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::RenameBasic);
    assert!(!run.requirements_satisfied());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::Unverified { .. }
    ));
    assert!(fixture.inner.is_empty());
}

#[test]
fn rename_rejection_without_final_existence_observation_is_unverified() {
    let fixture = ObservationFixture::new(false, None, None, None, Some(2), None, None);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::RenameBasic);
    assert!(!run.requirements_satisfied());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::Unverified { .. }
    ));
    assert!(fixture.inner.is_empty());
}

#[test]
fn rename_rejection_uses_confirmed_absence_when_seed_is_unsupported() {
    let fixture = ObservationFixture::new(true, None, None, None, None, None, None);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::RenameBasic);
    run.assert_satisfied();
    assert!(selected(run.report().checks()[0].outcome()));
    assert!(fixture.inner.is_empty());
}

#[test]
fn rename_rejection_observation_error_fails_after_preserving_error_validation() {
    let fixture = ObservationFixture::new(false, None, Some(2), None, None, None, None);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::RenameBasic);
    assert!(!run.requirements_satisfied());
    assert_eq!(
        run.failures()[0].check(),
        Some(ContractCheckId::RenameBasic)
    );
    assert!(fixture.inner.is_empty());
}

#[test]
fn rename_rejection_pre_observation_errors_fail_and_seed_gaps_are_unverified() {
    for fixture in [
        ObservationFixture::new(false, None, Some(1), None, None, None, None),
        ObservationFixture::new(true, None, None, None, None, None, Some(1)),
    ] {
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::RenameBasic);
        assert!(!run.requirements_satisfied());
        assert_eq!(
            run.failures()[0].check(),
            Some(ContractCheckId::RenameBasic)
        );
        assert!(fixture.inner.is_empty());
    }

    let fixture = ObservationFixture::new(true, None, None, None, Some(1), None, None);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::RenameBasic);
    assert!(!run.requirements_satisfied());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::Unverified { .. }
    ));
    assert!(fixture.inner.is_empty());
}

/// This mutates only the independent observation snapshot. It validates the
/// checker's detection path and does not claim an SPI side effect occurred.
struct ChangedSnapshotFixture {
    inner: MemoryFixture,
    reads: AtomicUsize,
}

impl FileSystemFixture for ChangedSnapshotFixture {
    fn file_system(&self) -> &qubit_fs::FileSystem {
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
        let snapshot = self.inner.read_file(path)?;
        if self.reads.fetch_add(1, Ordering::Relaxed) == 1 {
            return Ok(FixtureSupport::Supported(b"changed snapshot".to_vec()));
        }
        Ok(snapshot)
    }

    fn exists_out_of_band(&self, path: &Path) -> FixtureResult<FixtureSupport<bool>> {
        self.inner.exists_out_of_band(path)
    }
}

#[test]
fn rename_rejection_checker_detects_changed_independent_snapshot() {
    let fixture = ChangedSnapshotFixture {
        inner: MemoryFixture::without_operation_capabilities(),
        reads: AtomicUsize::new(0),
    };
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::RenameBasic);
    assert!(!run.requirements_satisfied());
    assert_eq!(
        run.failures()[0].check(),
        Some(ContractCheckId::RenameBasic)
    );
    assert!(fixture.inner.is_empty());
}

/// Checker self-validation only: the independent existence hook reports a new
/// target in its final snapshot; this does not claim a provider side effect.
#[test]
fn rename_rejection_checker_detects_target_appearance_in_independent_snapshot() {
    let fixture = ObservationFixture::new(false, None, None, None, None, Some(3), None);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::RenameBasic);
    assert!(!run.requirements_satisfied());
    assert_eq!(
        run.failures()[0].check(),
        Some(ContractCheckId::RenameBasic)
    );
    assert!(fixture.inner.is_empty());
}

#[cfg(feature = "async")]
mod asynchronous {
    use super::*;
    use common::AsyncMemoryFixture;
    use common::async_memory_file_system::run_controlled;
    use qubit_fs_testkit::AsyncFileSystemContractSuite;
    use qubit_fs_testkit::AsyncFileSystemFixture;
    use qubit_fs_testkit::FixtureFuture;

    struct AsyncObservationFixture {
        inner: AsyncMemoryFixture,
        no_seed: bool,
        unsupported_read_call: Option<usize>,
        failed_read_call: Option<usize>,
        changed_read_call: Option<usize>,
        unsupported_exists_call: Option<usize>,
        appeared_exists_call: Option<usize>,
        failed_exists_call: Option<usize>,
        read_calls: AtomicUsize,
        exists_calls: AtomicUsize,
    }

    impl AsyncObservationFixture {
        fn new(
            no_seed: bool,
            unsupported_read_call: Option<usize>,
            failed_read_call: Option<usize>,
            changed_read_call: Option<usize>,
            unsupported_exists_call: Option<usize>,
            appeared_exists_call: Option<usize>,
            failed_exists_call: Option<usize>,
        ) -> Self {
            Self {
                inner: AsyncMemoryFixture::without_operation_capabilities(),
                no_seed,
                unsupported_read_call,
                failed_read_call,
                changed_read_call,
                unsupported_exists_call,
                appeared_exists_call,
                failed_exists_call,
                read_calls: AtomicUsize::new(0),
                exists_calls: AtomicUsize::new(0),
            }
        }
    }

    impl AsyncFileSystemFixture for AsyncObservationFixture {
        fn file_system(&self) -> &qubit_fs::AsyncFileSystem {
            self.inner.file_system()
        }
        fn path(&self, relative: &str) -> FixtureResult<Path> {
            self.inner.path(relative)
        }
        fn teardown(&self) -> FixtureFuture<'_, ()> {
            self.inner.teardown()
        }
        fn seed_file<'a>(
            &'a self,
            relative: &'a str,
            bytes: &'a [u8],
        ) -> FixtureFuture<'a, FixtureSupport<Path>> {
            if self.no_seed {
                Box::pin(async { Ok(FixtureSupport::Unsupported) })
            } else {
                self.inner.seed_file(relative, bytes)
            }
        }
        fn read_file<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<Vec<u8>>> {
            let call = self.read_calls.fetch_add(1, Ordering::Relaxed) + 1;
            if self.failed_read_call == Some(call) {
                Box::pin(async {
                    Err(FixtureError::new("injected async read observation failure"))
                })
            } else if self.unsupported_read_call == Some(call) {
                Box::pin(async { Ok(FixtureSupport::Unsupported) })
            } else if self.changed_read_call == Some(call) {
                Box::pin(async { Ok(FixtureSupport::Supported(b"changed snapshot".to_vec())) })
            } else {
                self.inner.read_file(path)
            }
        }
        fn exists_out_of_band<'a>(
            &'a self,
            path: &'a Path,
        ) -> FixtureFuture<'a, FixtureSupport<bool>> {
            let call = self.exists_calls.fetch_add(1, Ordering::Relaxed) + 1;
            if self.failed_exists_call == Some(call) {
                Box::pin(async {
                    Err(FixtureError::new(
                        "injected async existence observation failure",
                    ))
                })
            } else if self.unsupported_exists_call == Some(call) {
                Box::pin(async { Ok(FixtureSupport::Unsupported) })
            } else if self.appeared_exists_call == Some(call) {
                Box::pin(async { Ok(FixtureSupport::Supported(true)) })
            } else {
                self.inner.exists_out_of_band(path)
            }
        }
    }

    #[test]
    fn async_rename_rejections_have_independent_resource_observations() {
        run_controlled(async {
            for (fixture, id, capability) in [
                (
                    AsyncMemoryFixture::without_operation_capabilities(),
                    ContractCheckId::RenameBasic,
                    FileSystemCapability::Rename,
                ),
                (
                    AsyncMemoryFixture::without_optional_capabilities(),
                    ContractCheckId::RenameAtomic,
                    FileSystemCapability::AtomicRename,
                ),
                (
                    AsyncMemoryFixture::without_optional_capabilities(),
                    ContractCheckId::RenameDurable,
                    FileSystemCapability::DurableRename,
                ),
            ] {
                assert!(
                    !fixture
                        .file_system()
                        .properties()
                        .capabilities()
                        .supports(capability)
                );
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                run.assert_satisfied();
                assert!(selected(run.report().checks()[0].outcome()), "{id}");
                assert!(fixture.is_empty());
                assert_eq!(
                    fixture.rename_spi_call_count(),
                    0,
                    "preflight must reject before SPI"
                );
            }
        });
    }

    #[test]
    fn async_rename_rejection_with_unsupported_final_observation_is_unverified() {
        let fixture = AsyncObservationFixture::new(false, None, None, None, Some(2), None, None);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = run_controlled(suite.run_check(ContractCheckId::RenameBasic));
        assert!(!run.requirements_satisfied());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { .. }
        ));
        assert!(fixture.inner.is_empty());
    }

    #[test]
    fn async_rename_rejection_uses_confirmed_absence_when_seed_is_unsupported() {
        let fixture = AsyncObservationFixture::new(true, None, None, None, None, None, None);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = run_controlled(suite.run_check(ContractCheckId::RenameBasic));
        run.assert_satisfied();
        assert!(selected(run.report().checks()[0].outcome()));
        assert!(fixture.inner.is_empty());
    }

    #[test]
    fn async_rename_rejection_observation_gap_and_error_are_classified() {
        let fixture = AsyncObservationFixture::new(false, Some(1), None, None, None, None, None);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = run_controlled(suite.run_check(ContractCheckId::RenameBasic));
        assert!(!run.requirements_satisfied());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { .. }
        ));
        assert!(fixture.inner.is_empty());

        let fixture = AsyncObservationFixture::new(false, None, Some(2), None, None, None, None);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = run_controlled(suite.run_check(ContractCheckId::RenameBasic));
        assert!(!run.requirements_satisfied());
        assert_eq!(
            run.failures()[0].check(),
            Some(ContractCheckId::RenameBasic)
        );
        assert!(fixture.inner.is_empty());
    }

    #[test]
    fn async_rename_rejection_pre_observation_errors_fail_and_seed_gaps_are_unverified() {
        for fixture in [
            AsyncObservationFixture::new(false, None, Some(1), None, None, None, None),
            AsyncObservationFixture::new(true, None, None, None, None, None, Some(1)),
        ] {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = run_controlled(suite.run_check(ContractCheckId::RenameBasic));
            assert!(!run.requirements_satisfied());
            assert_eq!(
                run.failures()[0].check(),
                Some(ContractCheckId::RenameBasic)
            );
            assert!(fixture.inner.is_empty());
        }

        let fixture = AsyncObservationFixture::new(true, None, None, None, Some(1), None, None);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = run_controlled(suite.run_check(ContractCheckId::RenameBasic));
        assert!(!run.requirements_satisfied());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { .. }
        ));
        assert!(fixture.inner.is_empty());
    }

    #[test]
    /// Checker self-validation only: the test mutates the independent snapshot
    /// hook's result and does not claim that provider SPI caused a change.
    fn async_rename_rejection_detects_changed_independent_snapshot() {
        let fixture = AsyncObservationFixture::new(false, None, None, Some(2), None, None, None);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = run_controlled(suite.run_check(ContractCheckId::RenameBasic));
        assert!(!run.requirements_satisfied());
        assert_eq!(
            run.failures()[0].check(),
            Some(ContractCheckId::RenameBasic)
        );
        assert!(fixture.inner.is_empty());
    }

    /// Checker self-validation only: the independent existence hook reports a
    /// new target; this does not claim the provider caused that change.
    #[test]
    fn async_rename_rejection_detects_target_appearance_in_independent_snapshot() {
        let fixture = AsyncObservationFixture::new(false, None, None, None, None, Some(3), None);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = run_controlled(suite.run_check(ContractCheckId::RenameBasic));
        assert!(!run.requirements_satisfied());
        assert_eq!(
            run.failures()[0].check(),
            Some(ContractCheckId::RenameBasic)
        );
        assert!(fixture.inner.is_empty());
    }
}
