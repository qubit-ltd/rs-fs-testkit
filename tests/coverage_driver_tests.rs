// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// Exercises contract-suite behavior through the public test-kit API.

use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
#[cfg(feature = "async")]
use std::sync::atomic::AtomicUsize;
#[cfg(feature = "async")]
use std::sync::atomic::Ordering;

pub use ::qubit_fs_testkit;
use qubit_fs::FileSystem;
use qubit_fs::copy::CopyMethod;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::path::Path;

#[cfg(feature = "async")]
use crate::common::AsyncMemoryFixture;
use crate::common::MemoryFixture;
#[cfg(feature = "async")]
use crate::common::async_memory_file_system::run_controlled;
#[cfg(feature = "async")]
use crate::qubit_fs_testkit::AsyncFileSystemContractSuite;
#[cfg(feature = "async")]
use crate::qubit_fs_testkit::AsyncFileSystemFixture;
use crate::qubit_fs_testkit::ContractCheckId;
use crate::qubit_fs_testkit::FileSystemContractSuite;
use crate::qubit_fs_testkit::FileSystemFixture;
#[cfg(feature = "async")]
use crate::qubit_fs_testkit::FixtureFuture;

mod common;

struct DefaultSyncFixture(MemoryFixture);

impl DefaultSyncFixture {
    fn new() -> Self {
        Self(MemoryFixture::new())
    }
}

impl FileSystemFixture for DefaultSyncFixture {
    fn file_system(&self) -> &FileSystem {
        self.0.file_system()
    }

    fn path(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<Path> {
        self.0.path(relative)
    }

    fn teardown(&self) -> qubit_fs_testkit::FixtureResult<()> {
        self.0.teardown()
    }
}

struct TempOnlySyncFixture(MemoryFixture);

impl FileSystemFixture for TempOnlySyncFixture {
    fn file_system(&self) -> &FileSystem {
        self.0.file_system()
    }

    fn path(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<Path> {
        self.0.path(relative)
    }

    fn exists_out_of_band(
        &self,
        path: &Path,
    ) -> qubit_fs_testkit::FixtureResult<qubit_fs_testkit::FixtureSupport<bool>> {
        self.0.exists_out_of_band(path)
    }

    fn seed_empty_directory(
        &self,
        relative: &str,
    ) -> qubit_fs_testkit::FixtureResult<qubit_fs_testkit::FixtureSupport<Path>> {
        self.0.seed_empty_directory(relative)
    }

    fn teardown(&self) -> qubit_fs_testkit::FixtureResult<()> {
        self.0.teardown()
    }
}

struct SyncReadUnsupportedFixture(MemoryFixture);

impl FileSystemFixture for SyncReadUnsupportedFixture {
    fn file_system(&self) -> &FileSystem {
        self.0.file_system()
    }

    fn path(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<Path> {
        self.0.path(relative)
    }

    fn seed_file(
        &self,
        relative: &str,
        bytes: &[u8],
    ) -> qubit_fs_testkit::FixtureResult<qubit_fs_testkit::FixtureSupport<Path>> {
        self.0.seed_file(relative, bytes)
    }

    fn read_file(&self, path: &Path) -> qubit_fs_testkit::FixtureResult<qubit_fs_testkit::FixtureSupport<Vec<u8>>> {
        if path.as_str().ends_with("/nested/sentinel") {
            Ok(qubit_fs_testkit::FixtureSupport::Unsupported)
        } else {
            self.0.read_file(path)
        }
    }

    fn exists_out_of_band(
        &self,
        path: &Path,
    ) -> qubit_fs_testkit::FixtureResult<qubit_fs_testkit::FixtureSupport<bool>> {
        self.0.exists_out_of_band(path)
    }

    fn write_file_out_of_band(
        &self,
        path: &Path,
        bytes: &[u8],
    ) -> qubit_fs_testkit::FixtureResult<qubit_fs_testkit::FixtureSupport<()>> {
        self.0.write_file_out_of_band(path, bytes)
    }

    fn seed_empty_directory(
        &self,
        relative: &str,
    ) -> qubit_fs_testkit::FixtureResult<qubit_fs_testkit::FixtureSupport<Path>> {
        self.0.seed_empty_directory(relative)
    }

    fn teardown(&self) -> qubit_fs_testkit::FixtureResult<()> {
        self.0.teardown()
    }
}

#[cfg(feature = "async")]
struct DefaultAsyncFixture(AsyncMemoryFixture);

#[cfg(feature = "async")]
impl DefaultAsyncFixture {
    fn new() -> Self {
        Self(AsyncMemoryFixture::new())
    }
}

#[cfg(feature = "async")]
impl AsyncFileSystemFixture for DefaultAsyncFixture {
    fn file_system(&self) -> &qubit_fs::AsyncFileSystem {
        self.0.file_system()
    }

    fn path(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<Path> {
        self.0.path(relative)
    }

    fn teardown(&self) -> FixtureFuture<'_, ()> {
        self.0.teardown()
    }
}

#[cfg(feature = "async")]
struct TempOnlyAsyncFixture(AsyncMemoryFixture);

#[cfg(feature = "async")]
impl AsyncFileSystemFixture for TempOnlyAsyncFixture {
    fn file_system(&self) -> &qubit_fs::AsyncFileSystem {
        self.0.file_system()
    }

    fn path(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<Path> {
        self.0.path(relative)
    }

    fn exists_out_of_band<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, qubit_fs_testkit::FixtureSupport<bool>> {
        self.0.exists_out_of_band(path)
    }

    fn seed_empty_directory<'a>(
        &'a self,
        relative: &'a str,
    ) -> FixtureFuture<'a, qubit_fs_testkit::FixtureSupport<Path>> {
        self.0.seed_empty_directory(relative)
    }

    fn teardown(&self) -> FixtureFuture<'_, ()> {
        self.0.teardown()
    }
}

#[cfg(feature = "async")]
struct AsyncReadUnsupportedFixture(AsyncMemoryFixture);

#[cfg(feature = "async")]
impl AsyncFileSystemFixture for AsyncReadUnsupportedFixture {
    fn file_system(&self) -> &qubit_fs::AsyncFileSystem {
        self.0.file_system()
    }

    fn path(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<Path> {
        self.0.path(relative)
    }

    fn seed_file<'a>(
        &'a self,
        relative: &'a str,
        bytes: &'a [u8],
    ) -> FixtureFuture<'a, qubit_fs_testkit::FixtureSupport<Path>> {
        self.0.seed_file(relative, bytes)
    }

    fn read_file<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, qubit_fs_testkit::FixtureSupport<Vec<u8>>> {
        if path.as_str().ends_with("/nested/sentinel") {
            Box::pin(async { Ok(qubit_fs_testkit::FixtureSupport::Unsupported) })
        } else {
            self.0.read_file(path)
        }
    }

    fn exists_out_of_band<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, qubit_fs_testkit::FixtureSupport<bool>> {
        self.0.exists_out_of_band(path)
    }

    fn write_file_out_of_band<'a>(
        &'a self,
        path: &'a Path,
        bytes: &'a [u8],
    ) -> FixtureFuture<'a, qubit_fs_testkit::FixtureSupport<()>> {
        self.0.write_file_out_of_band(path, bytes)
    }

    fn seed_empty_directory<'a>(
        &'a self,
        relative: &'a str,
    ) -> FixtureFuture<'a, qubit_fs_testkit::FixtureSupport<Path>> {
        self.0.seed_empty_directory(relative)
    }

    fn teardown(&self) -> FixtureFuture<'_, ()> {
        self.0.teardown()
    }
}

#[cfg(feature = "async")]
struct AsyncDeleteTreeObservationGapFixture {
    inner: AsyncMemoryFixture,
    observations: AtomicUsize,
    unsupported_after: usize,
    directories_supported: bool,
}

#[cfg(feature = "async")]
impl AsyncFileSystemFixture for AsyncDeleteTreeObservationGapFixture {
    fn file_system(&self) -> &qubit_fs::AsyncFileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn seed_file<'a>(
        &'a self,
        relative: &'a str,
        bytes: &'a [u8],
    ) -> FixtureFuture<'a, qubit_fs_testkit::FixtureSupport<Path>> {
        self.inner.seed_file(relative, bytes)
    }

    fn seed_empty_directory<'a>(
        &'a self,
        relative: &'a str,
    ) -> FixtureFuture<'a, qubit_fs_testkit::FixtureSupport<Path>> {
        if self.directories_supported {
            self.inner.seed_empty_directory(relative)
        } else {
            Box::pin(async { Ok(qubit_fs_testkit::FixtureSupport::Unsupported) })
        }
    }

    fn exists_out_of_band<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, qubit_fs_testkit::FixtureSupport<bool>> {
        if self.observations.fetch_add(1, Ordering::Relaxed) < self.unsupported_after {
            self.inner.exists_out_of_band(path)
        } else {
            Box::pin(async { Ok(qubit_fs_testkit::FixtureSupport::Unsupported) })
        }
    }

    fn teardown(&self) -> FixtureFuture<'_, ()> {
        self.inner.teardown()
    }
}

#[cfg(feature = "async")]
#[derive(Clone, Copy)]
enum AsyncReadEvidenceGap {
    Preparation,
    CurrentVersion,
    StaleVersion,
}

#[cfg(feature = "async")]
struct AsyncReadEvidenceGapFixture {
    inner: AsyncMemoryFixture,
    gap: AsyncReadEvidenceGap,
}

#[cfg(feature = "async")]
impl AsyncFileSystemFixture for AsyncReadEvidenceGapFixture {
    fn file_system(&self) -> &qubit_fs::AsyncFileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn prepare_read<'a>(
        &'a self,
        scenario: crate::qubit_fs_testkit::ReadScenario,
        relative: &'a str,
        bytes: &'a [u8],
    ) -> FixtureFuture<'a, crate::qubit_fs_testkit::FixturePreparation<Path>> {
        if matches!(self.gap, AsyncReadEvidenceGap::Preparation) {
            return Box::pin(async {
                Ok(crate::qubit_fs_testkit::FixturePreparation::NotApplicable {
                    reason: "the fixture deliberately lacks this read scenario".to_owned(),
                })
            });
        }
        self.inner.prepare_read(scenario, relative, bytes)
    }

    fn seed_file<'a>(
        &'a self,
        relative: &'a str,
        bytes: &'a [u8],
    ) -> FixtureFuture<'a, qubit_fs_testkit::FixtureSupport<Path>> {
        self.inner.seed_file(relative, bytes)
    }

    fn resource_version<'a>(
        &'a self,
        path: &'a Path,
    ) -> FixtureFuture<'a, qubit_fs_testkit::FixtureSupport<qubit_fs::metadata::ResourceVersion>> {
        if matches!(self.gap, AsyncReadEvidenceGap::CurrentVersion) {
            return Box::pin(async { Ok(qubit_fs_testkit::FixtureSupport::Unsupported) });
        }
        self.inner.resource_version(path)
    }

    fn stale_resource_version<'a>(
        &'a self,
        path: &'a Path,
    ) -> FixtureFuture<'a, qubit_fs_testkit::FixtureSupport<qubit_fs::metadata::ResourceVersion>> {
        if matches!(self.gap, AsyncReadEvidenceGap::StaleVersion) {
            return Box::pin(async { Ok(qubit_fs_testkit::FixtureSupport::Unsupported) });
        }
        self.inner.stale_resource_version(path)
    }

    fn teardown(&self) -> FixtureFuture<'_, ()> {
        self.inner.teardown()
    }
}

#[test]
fn test_synchronous_suite_handles_default_unsupported_fixture_hooks() {
    let fixture = DefaultSyncFixture::new();
    assert!(matches!(
        fixture.seed_file("coverage-seed", b"payload"),
        Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
    ));
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_all();
    assert!(!run.was_interrupted());
    assert!(run.failures().iter().all(|failure| failure.check().is_some()));
    assert!(run.report().checks().iter().any(|check| matches!(
        check.outcome(),
        crate::qubit_fs_testkit::ContractCheckOutcome::Unverified { .. }
    )));
    assert!(run.cleanup().completed());
}

#[test]
fn test_synchronous_temp_capabilities_without_observation_hooks_remain_unverified() {
    use crate::qubit_fs_testkit::ContractCheckOutcome;
    use crate::qubit_fs_testkit::FileSystemContract;

    let fixture = TempOnlySyncFixture(MemoryFixture::temp_only());
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::TempResources);

    assert!(!run.was_interrupted());
    assert!(run.failures().is_empty(), "{:?}", run.failures());
    assert!(
        run.report()
            .checks()
            .iter()
            .any(|check| matches!(check.outcome(), ContractCheckOutcome::Unverified { .. }))
    );
    assert!(run.cleanup().completed());
    assert!(
        fixture.0.is_empty(),
        "temporary suite must clean up without hook support"
    );
}

#[test]
fn test_sync_temp_lifecycles_record_unsupported_observation_hooks() {
    use crate::qubit_fs_testkit::ContractCheckOutcome;

    for id in [
        crate::qubit_fs_testkit::ContractCheckId::TempFile,
        crate::qubit_fs_testkit::ContractCheckId::TempDirectory,
        crate::qubit_fs_testkit::ContractCheckId::TempRepeatedLifecycle,
    ] {
        let fixture = TempOnlySyncFixture(MemoryFixture::with_all_capabilities());
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        assert!(run.failures().is_empty(), "{id}: {:?}", run.failures());
        assert!(
            run.report().checks().iter().any(|check| {
                check.id() == id && matches!(check.outcome(), ContractCheckOutcome::Unverified { .. })
            })
        );
        assert!(run.cleanup().completed(), "{id} cleanup must complete");
        assert!(fixture.0.is_empty(), "{id} must clean up the underlying provider");
    }
}

#[test]
fn test_sync_temp_publication_reports_unsupported_read_evidence() {
    use crate::qubit_fs_testkit::ContractCheckOutcome;

    let id = ContractCheckId::TempRepeatedLifecycle;
    {
        let fixture = SyncReadUnsupportedFixture(MemoryFixture::with_all_capabilities());
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        assert!(run.failures().is_empty(), "{id}: {:?}", run.failures());
        assert!(
            run.report().checks().iter().any(|check| {
                check.id() == id && matches!(check.outcome(), ContractCheckOutcome::Unverified { .. })
            })
        );
        assert!(run.cleanup().completed(), "{id} cleanup must complete");
        assert!(fixture.0.is_empty(), "{id} must clean up the underlying provider");
    }
}

#[test]
fn test_sync_repeated_temp_lifecycle_detects_publication_faults() {
    for fault in [
        crate::common::MemoryFault::TempKeepDropsFileContents,
        crate::common::MemoryFault::TempKeepDropsDirectoryChildren,
        crate::common::MemoryFault::TempReusesLivePath,
        crate::common::MemoryFault::TempCleanupFailsOnce,
    ] {
        let fixture = MemoryFixture::with_fault(fault);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempRepeatedLifecycle);
        assert!(
            !run.requirements_satisfied(),
            "{fault:?} must violate repeated lifecycle"
        );
        assert!(
            run.failures()
                .iter()
                .all(|failure| { failure.check() == Some(ContractCheckId::TempRepeatedLifecycle) })
        );
        suite.finish();
        assert!(fixture.is_empty(), "{fault:?} must clean up the fixture");
    }
}

#[test]
fn test_synchronous_fixture_defaults_return_explicit_unsupported_evidence() {
    use crate::qubit_fs_testkit::CopyScenario;
    use crate::qubit_fs_testkit::DeleteScenario;
    use crate::qubit_fs_testkit::ReadScenario;
    use crate::qubit_fs_testkit::WriteScenario;

    let fixture = DefaultSyncFixture::new();
    let path = fixture.path("default-hook").expect("valid fixture path");
    assert!(!fixture.copy_fallback_only());
    assert_eq!(fixture.list_prefix(&path, "child").expect("prefix"), "child");
    assert!(matches!(
        fixture.snapshot_namespace_paths(),
        Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.seed_file("file", b"bytes"),
        Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.read_file(&path),
        Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.exists_out_of_band(&path),
        Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.write_file_out_of_band(&path, b"bytes"),
        Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.resource_version(&path),
        Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.stale_resource_version(&path),
        Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.checksum_failure_case("file"),
        Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.seed_empty_directory("directory"),
        Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.seed_symlink("link"),
        Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.copy_fast_path_case(CopyMethod::ServerSide),
        Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
    ));

    for scenario in [
        CopyScenario::Basic,
        CopyScenario::AtomicFile,
        CopyScenario::DurableFile,
        CopyScenario::AtomicTree,
        CopyScenario::DurableTree,
        CopyScenario::ServerSide,
        CopyScenario::Conflict,
    ] {
        assert!(matches!(
            fixture.prepare_copy(scenario, "source", "target", b"bytes"),
            Ok(crate::qubit_fs_testkit::FixturePreparation::Unavailable { .. })
        ));
    }
    assert!(matches!(
        fixture.prepare_delete(DeleteScenario::Basic, "file", b"bytes"),
        Ok(crate::qubit_fs_testkit::FixturePreparation::Unavailable { .. })
    ));
    for scenario in [
        ReadScenario::Basic,
        ReadScenario::Range,
        ReadScenario::RangeLimit,
        ReadScenario::IfMatchCurrent,
        ReadScenario::IfMatchStale,
        ReadScenario::IfNoneMatchCurrent,
        ReadScenario::IfNoneMatchStale,
        ReadScenario::Checksum,
        ReadScenario::ChecksumCorruption,
    ] {
        assert!(matches!(
            fixture.prepare_read(scenario, "file", b"bytes"),
            Ok(crate::qubit_fs_testkit::FixturePreparation::Unavailable { .. })
        ));
    }
    for scenario in [
        WriteScenario::CreateConflict,
        WriteScenario::Replace,
        WriteScenario::Append,
        WriteScenario::IfAbsent,
        WriteScenario::IfMatch,
        WriteScenario::AtomicReplace,
    ] {
        assert!(matches!(
            fixture.prepare_write(scenario, "file", b"bytes"),
            Ok(crate::qubit_fs_testkit::FixturePreparation::Unavailable { .. })
        ));
    }
    for scenario in [WriteScenario::Create, WriteScenario::Abort, WriteScenario::Durable] {
        assert!(matches!(
            fixture.prepare_write(scenario, "file", b"bytes"),
            Ok(crate::qubit_fs_testkit::FixturePreparation::Ready(_))
        ));
    }
}

#[test]
fn test_synchronous_advertised_capability_requires_seed_support() {
    let fixture = MemoryFixture::with_unavailable_seed_calls(None, Some(1));
    let mut suite = FileSystemContractSuite::new(&fixture);
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _seed = suite.required_seed("required-seed", b"payload", "coverage probe");
    }));
    assert!(result.is_err());
}

#[test]
fn test_sync_read_checks_report_unavailable_independent_seeds() {
    use crate::qubit_fs_testkit::ContractCheckOutcome;

    for id in [
        ContractCheckId::ReadBasic,
        ContractCheckId::ReadChecksum,
        ContractCheckId::ReadChecksumCorruption,
        ContractCheckId::ReadIfMatchCurrent,
        ContractCheckId::ReadIfMatchStale,
        ContractCheckId::ReadIfNoneMatchCurrent,
        ContractCheckId::ReadIfNoneMatchStale,
        ContractCheckId::ReadRange,
        ContractCheckId::ReadRangeLimit,
    ] {
        let fixture = MemoryFixture::with_unavailable_seed_calls(None, Some(1));
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        assert!(run.failures().is_empty(), "{id}: {:?}", run.failures());
        assert!(
            matches!(
                run.report().checks()[0].outcome(),
                ContractCheckOutcome::Unverified { .. }
                    | ContractCheckOutcome::SkippedOptional { .. }
                    | ContractCheckOutcome::RejectedAsExpected
            ),
            "{id}: {:?}",
            run.report().checks()[0].outcome()
        );
        assert!(run.cleanup().completed(), "{id} cleanup must complete");
    }
}

#[test]
fn test_sync_recursive_delete_reports_missing_independent_child_seed() {
    use crate::qubit_fs_testkit::ContractCheckOutcome;

    let fixture = TempOnlySyncFixture(MemoryFixture::with_all_capabilities());
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::DeleteTree);
    assert!(run.failures().is_empty(), "{:?}", run.failures());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::Unverified { .. }
    ));
    assert!(run.cleanup().completed());
    assert!(fixture.0.is_empty());
}

#[test]
fn test_synchronous_copy_checks_reject_unadvertised_guarantees() {
    for id in [
        ContractCheckId::CopyServerSide,
        ContractCheckId::CopyAtomicFile,
        ContractCheckId::CopyDurableFile,
    ] {
        let fixture = MemoryFixture::new();
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        assert!(run.requirements_satisfied(), "{id}: {:?}", run.failures());
        assert!(fixture.is_empty(), "{id}: fixture leaked resources");
    }
}

#[test]
fn test_synchronous_server_side_copy_reports_unavailable_fixture_evidence() {
    let fixture = MemoryFixture::with_conditional_case_unavailable(crate::common::UnavailableScenario::Capability(
        FileSystemCapability::ServerSideCopy,
    ));
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::CopyServerSide);
    assert!(!run.was_interrupted());
    assert!(run.failures().is_empty());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        crate::qubit_fs_testkit::ContractCheckOutcome::Unverified { .. }
    ));
    assert!(run.cleanup().completed());
}

#[test]
fn test_synchronous_server_side_copy_rejects_an_unadvertised_capability() {
    let fixture = MemoryFixture::without_optional_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::CopyServerSide);
    assert!(!run.was_interrupted());
    assert!(run.failures().is_empty());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        crate::qubit_fs_testkit::ContractCheckOutcome::RejectedAsExpected
    ));
}

#[test]
fn test_synchronous_temp_directory_paths_retain_creation_and_cleanup_failures() {
    for (fault, id) in [
        (
            crate::common::MemoryFault::TempCreationFails,
            ContractCheckId::TempDirectory,
        ),
        (
            crate::common::MemoryFault::TempCleanupFailsOnce,
            ContractCheckId::TempDirectory,
        ),
        (
            crate::common::MemoryFault::TempKeepFails,
            ContractCheckId::TempRepeatedLifecycle,
        ),
    ] {
        let fixture = MemoryFixture::with_fault(fault);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        assert!(!run.failures().is_empty(), "{fault:?} must fail {id}");
        assert!(run.failures().iter().all(|failure| failure.check() == Some(id)));
        assert!(fixture.is_empty(), "{fault:?} must clean up the fixture");
    }
}

#[test]
fn test_synchronous_expected_write_rejection_cleanup_errors_are_retained() {
    for (fault, id) in [
        (
            crate::common::MemoryFault::ConditionalAbortFailsOnce,
            ContractCheckId::WriteCreateConflict,
        ),
        (
            crate::common::MemoryFault::ConditionalAbortPanicsOnce,
            ContractCheckId::WriteIfAbsent,
        ),
        (
            crate::common::MemoryFault::ConditionalAbortReportsPublished,
            ContractCheckId::WriteIfAbsent,
        ),
    ] {
        let fixture = MemoryFixture::with_fault(fault);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        assert!(
            run.failures().iter().any(|failure| failure.check() == Some(id)),
            "{fault:?} must be attributed to {id}"
        );
        assert!(fixture.is_empty(), "{fault:?} must clean up the fixture");
    }
}

#[test]
fn test_sync_temp_contracts_retain_indexed_fixture_failures() {
    use crate::common::shared_model::FixtureHook;

    for id in [
        ContractCheckId::TempFile,
        ContractCheckId::TempDirectory,
        ContractCheckId::TempAtomic,
        ContractCheckId::TempRepeatedLifecycle,
    ] {
        let baseline = MemoryFixture::with_all_capabilities();
        let mut suite = FileSystemContractSuite::new(&baseline);
        let run = suite.run_check(id);
        assert!(run.requirements_satisfied(), "baseline {id}: {:?}", run.failures());
        let counts = (
            baseline.path_call_count(),
            baseline.exists_call_count(),
            baseline.seed_file_call_count(),
            baseline.seed_directory_call_count(),
            baseline.temp_creation_call_count(),
            baseline.fixture_hook_call_count(FixtureHook::ReadFile),
            baseline.fixture_hook_call_count(FixtureHook::WriteFile),
        );
        assert!(baseline.is_empty(), "baseline {id} must clean up");

        let mut cases = Vec::new();
        cases.extend((1..=counts.0).map(MemoryFixture::with_path_error_call));
        cases.extend((1..=counts.1).map(MemoryFixture::with_exists_error_call));
        cases.extend((1..=counts.2).map(|call| MemoryFixture::with_seed_error_calls(Some(call), None)));
        cases.extend((1..=counts.3).map(|call| MemoryFixture::with_seed_error_calls(None, Some(call))));
        cases.extend((1..=counts.4).map(MemoryFixture::with_temp_creation_error_call));
        cases.extend((1..=counts.5).map(|call| MemoryFixture::with_fixture_hook_error(FixtureHook::ReadFile, call)));
        cases.extend((1..=counts.6).map(|call| MemoryFixture::with_fixture_hook_error(FixtureHook::WriteFile, call)));

        for fixture in cases {
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id);
            assert!(
                run.failures().iter().any(|failure| failure.check() == Some(id)),
                "an indexed fixture failure must be attributed to {id}"
            );
            assert!(
                run.cleanup().completed(),
                "indexed fixture failure in {id} must clean up"
            );
            assert!(fixture.is_empty(), "indexed fixture failure in {id} leaked resources");
        }
    }
}

#[test]
fn test_sync_read_contracts_retain_indexed_fixture_failures() {
    use crate::common::shared_model::FixtureHook;

    let hooks = [
        FixtureHook::ReadFile,
        FixtureHook::ResourceVersion,
        FixtureHook::StaleResourceVersion,
        FixtureHook::ChecksumFailureCase,
    ];
    for id in [
        ContractCheckId::ReadBasic,
        ContractCheckId::ReadChecksum,
        ContractCheckId::ReadChecksumCorruption,
        ContractCheckId::ReadIfMatchCurrent,
        ContractCheckId::ReadIfMatchStale,
        ContractCheckId::ReadIfNoneMatchCurrent,
        ContractCheckId::ReadIfNoneMatchStale,
        ContractCheckId::ReadRange,
        ContractCheckId::ReadRangeLimit,
    ] {
        let baseline = MemoryFixture::with_all_capabilities();
        let mut suite = FileSystemContractSuite::new(&baseline);
        let run = suite.run_check(id);
        assert!(run.requirements_satisfied(), "baseline {id}: {:?}", run.failures());
        let path_calls = baseline.path_call_count();
        let seed_calls = baseline.seed_file_call_count();
        let hook_calls = hooks.map(|hook| (hook, baseline.fixture_hook_call_count(hook)));
        assert!(baseline.is_empty(), "baseline {id} must clean up");

        let mut cases = Vec::new();
        cases.extend((1..=path_calls).map(MemoryFixture::with_path_error_call));
        cases.extend((1..=seed_calls).map(|call| MemoryFixture::with_seed_error_calls(Some(call), None)));
        for (hook, calls) in hook_calls {
            cases.extend((1..=calls).map(|call| MemoryFixture::with_fixture_hook_error(hook, call)));
        }

        for fixture in cases {
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id);
            assert!(
                run.failures().iter().any(|failure| failure.check() == Some(id)),
                "an indexed fixture failure must be attributed to {id}"
            );
            assert!(
                run.cleanup().completed(),
                "indexed fixture failure in {id} must clean up"
            );
            assert!(fixture.is_empty(), "indexed fixture failure in {id} leaked resources");
        }
    }
}

#[test]
fn test_temporary_directories_reject_atomic_persist_when_not_advertised() {
    let fixture = MemoryFixture::without_atomic_temp_persist();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::TempAtomic);
    assert!(run.requirements_satisfied(), "{:?}", run.failures());
    assert!(fixture.is_empty(), "atomic rejection must clean up the sync fixture");

    let baseline = MemoryFixture::without_atomic_temp_persist();
    let mut suite = FileSystemContractSuite::new(&baseline);
    let _ = suite.run_check(ContractCheckId::TempAtomic);
    for call in 1..=baseline.exists_call_count() {
        let fixture = MemoryFixture::without_atomic_temp_persist_with_exists_error_call(call);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempAtomic);
        assert!(!run.failures().is_empty(), "sync observation {call} must fail");
        assert!(
            run.failures()
                .iter()
                .all(|failure| failure.check() == Some(ContractCheckId::TempAtomic))
        );
        assert!(fixture.is_empty(), "sync observation {call} must clean up");
    }

    let fixture = MemoryFixture::with_all_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::TempRepeatedLifecycle);
    assert!(run.requirements_satisfied(), "{:?}", run.failures());
    assert!(fixture.is_empty(), "repeated lifecycle must clean up the sync fixture");

    #[cfg(feature = "async")]
    {
        let fixture = AsyncMemoryFixture::without_atomic_temp_persist();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::TempAtomic).await;
            assert!(run.requirements_satisfied(), "{:?}", run.failures());
        });
        assert!(fixture.is_empty(), "atomic rejection must clean up the async fixture");

        let baseline = AsyncMemoryFixture::without_atomic_temp_persist();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&baseline);
            let _ = suite.run_check(ContractCheckId::TempAtomic).await;
        });
        for call in 1..=baseline.exists_call_count() {
            let fixture = AsyncMemoryFixture::without_atomic_temp_persist_with_exists_error_call(call);
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(ContractCheckId::TempAtomic).await;
                assert!(!run.failures().is_empty(), "async observation {call} must fail");
                assert!(
                    run.failures()
                        .iter()
                        .all(|failure| { failure.check() == Some(ContractCheckId::TempAtomic) })
                );
            });
            assert!(fixture.is_empty(), "async observation {call} must clean up");
        }

        let fixture = AsyncMemoryFixture::with_all_capabilities();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::TempRepeatedLifecycle).await;
            assert!(run.requirements_satisfied(), "{:?}", run.failures());
        });
        assert!(fixture.is_empty(), "repeated lifecycle must clean up the async fixture");
    }
}

#[test]
fn test_sync_temp_directory_does_not_require_directory_creation_capability() {
    let fixture = MemoryFixture::temp_only();
    assert!(
        !fixture
            .file_system()
            .properties()
            .capabilities()
            .supports(FileSystemCapability::CreateDirectory)
    );
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::TempDirectory);
    assert!(run.requirements_satisfied(), "{:?}", run.failures());
    assert!(fixture.is_empty());
}

#[test]
fn test_synchronous_core_profile_preserves_path_failures() {
    for id in ContractCheckId::ALL
        .iter()
        .copied()
        .filter(|id| id.supports_synchronous())
    {
        let baseline = MemoryFixture::without_optional_capabilities();
        let mut suite = FileSystemContractSuite::new(&baseline);
        let _ = suite.run_check(id);
        let path_calls = baseline.path_call_count();
        if path_calls == 0 {
            continue;
        }
        for call in 1..=path_calls {
            let fixture = MemoryFixture::without_optional_capabilities_with_path_error_call(call);
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id);
            assert!(!run.failures().is_empty(), "{id}, path {call}");
            assert!(
                run.failures().iter().all(|failure| failure.check() == Some(id)),
                "{id}, path {call}"
            );
            assert!(fixture.is_empty(), "{id}, path {call}: fixture leaked resources");
        }
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_suite_handles_default_unsupported_fixture_hooks() {
    let fixture = DefaultAsyncFixture::new();
    assert!(matches!(
        run_controlled(async { fixture.seed_file("coverage-seed", b"payload").await }),
        Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
    ));
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_all().await;
        assert!(!run.was_interrupted());
        assert!(run.failures().iter().all(|failure| failure.check().is_some()));
        assert!(run.cleanup().completed());
    });
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_temp_capabilities_without_observation_hooks_remain_unverified() {
    use crate::qubit_fs_testkit::ContractCheckOutcome;
    use crate::qubit_fs_testkit::FileSystemContract;

    let fixture = TempOnlyAsyncFixture(AsyncMemoryFixture::temp_only());
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(FileSystemContract::TempResources).await;

        assert!(!run.was_interrupted());
        assert!(run.failures().is_empty(), "{:?}", run.failures());
        assert!(
            run.report()
                .checks()
                .iter()
                .any(|check| matches!(check.outcome(), ContractCheckOutcome::Unverified { .. }))
        );
        assert!(run.cleanup().completed());
    });
    assert!(
        fixture.0.is_empty(),
        "temporary suite must clean up without hook support"
    );
}

#[cfg(feature = "async")]
#[test]
fn test_async_temp_lifecycles_record_unsupported_observation_hooks() {
    use crate::qubit_fs_testkit::ContractCheckOutcome;

    let id = ContractCheckId::TempRepeatedLifecycle;
    {
        let fixture = TempOnlyAsyncFixture(AsyncMemoryFixture::with_all_capabilities());
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(run.failures().is_empty(), "{id}: {:?}", run.failures());
            assert!(run.report().checks().iter().any(|check| {
                check.id() == id && matches!(check.outcome(), ContractCheckOutcome::Unverified { .. })
            }));
            assert!(run.cleanup().completed(), "{id} cleanup must complete");
        });
        assert!(fixture.0.is_empty(), "{id} must clean up the underlying provider");
    }
}

#[cfg(feature = "async")]
#[test]
fn test_async_temp_publication_reports_unsupported_read_evidence() {
    use crate::qubit_fs_testkit::ContractCheckOutcome;

    let id = ContractCheckId::TempRepeatedLifecycle;
    {
        let fixture = AsyncReadUnsupportedFixture(AsyncMemoryFixture::with_all_capabilities());
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(run.failures().is_empty(), "{id}: {:?}", run.failures());
            assert!(run.report().checks().iter().any(|check| {
                check.id() == id && matches!(check.outcome(), ContractCheckOutcome::Unverified { .. })
            }));
            assert!(run.cleanup().completed(), "{id} cleanup must complete");
        });
        assert!(fixture.0.is_empty(), "{id} must clean up the underlying provider");
    }
}

#[cfg(feature = "async")]
#[test]
fn test_async_repeated_temp_lifecycle_detects_publication_faults() {
    for fault in [
        crate::common::AsyncMemoryFault::TempKeepDropsFileContents,
        crate::common::AsyncMemoryFault::TempKeepDropsDirectoryChildren,
        crate::common::AsyncMemoryFault::TempReusesLivePath,
        crate::common::AsyncMemoryFault::TempCleanupFailsOnce,
    ] {
        let fixture = AsyncMemoryFixture::with_fault(fault);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::TempRepeatedLifecycle).await;
            assert!(
                !run.requirements_satisfied(),
                "{fault:?} must violate repeated lifecycle"
            );
            assert!(
                run.failures()
                    .iter()
                    .all(|failure| { failure.check() == Some(ContractCheckId::TempRepeatedLifecycle) })
            );
            suite.finish().await;
        });
        assert!(fixture.is_empty(), "{fault:?} must clean up the fixture");
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_advertised_capability_requires_seed_support() {
    let fixture = AsyncMemoryFixture::with_unavailable_seed_calls(None, Some(1));
    let result = catch_unwind(AssertUnwindSafe(|| {
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let _seed = suite.required_seed("required-seed", b"payload", "coverage probe").await;
        });
    }));
    assert!(result.is_err());
}

#[cfg(feature = "async")]
#[test]
fn test_async_read_checks_report_unavailable_independent_seeds() {
    use crate::qubit_fs_testkit::ContractCheckOutcome;

    for id in [
        ContractCheckId::ReadBasic,
        ContractCheckId::ReadChecksum,
        ContractCheckId::ReadChecksumCorruption,
        ContractCheckId::ReadIfMatchCurrent,
        ContractCheckId::ReadIfMatchStale,
        ContractCheckId::ReadIfNoneMatchCurrent,
        ContractCheckId::ReadIfNoneMatchStale,
        ContractCheckId::ReadRange,
        ContractCheckId::ReadRangeLimit,
    ] {
        let fixture = AsyncMemoryFixture::with_unavailable_seed_calls(None, Some(1));
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(run.failures().is_empty(), "{id}: {:?}", run.failures());
            assert!(
                matches!(
                    run.report().checks()[0].outcome(),
                    ContractCheckOutcome::Unverified { .. }
                        | ContractCheckOutcome::SkippedOptional { .. }
                        | ContractCheckOutcome::RejectedAsExpected
                ),
                "{id}: {:?}",
                run.report().checks()[0].outcome()
            );
            assert!(run.cleanup().completed(), "{id} cleanup must complete");
        });
        assert!(fixture.is_empty(), "{id} must not leak fixture resources");
    }
}

#[cfg(feature = "async")]
#[test]
fn test_async_recursive_delete_reports_missing_independent_child_seed() {
    use crate::qubit_fs_testkit::ContractCheckOutcome;

    let fixture = TempOnlyAsyncFixture(AsyncMemoryFixture::with_all_capabilities());
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::DeleteTree).await;
        assert!(run.failures().is_empty(), "{:?}", run.failures());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { .. }
        ));
        assert!(run.cleanup().completed());
    });
    assert!(fixture.0.is_empty());
}

#[cfg(feature = "async")]
#[test]
fn test_async_bounded_read_range_and_caller_limit_paths() {
    use qubit_fs::metadata::FileSystemLimit;
    use qubit_fs::metadata::FileSystemLimits;

    let fixture = AsyncMemoryFixture::with_fault_and_limits(
        crate::common::AsyncMemoryFault::None,
        FileSystemLimits::unknown().with_max_read_range_bytes(FileSystemLimit::Maximum(4)),
    );
    for id in [
        ContractCheckId::ReadBasic,
        ContractCheckId::ReadRange,
        ContractCheckId::ReadRangeLimit,
    ] {
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(run.requirements_satisfied(), "{id}: {:?}", run.failures());
        });
        assert!(fixture.is_empty(), "{id} must clean up seeded reads");
    }
}

#[cfg(feature = "async")]
#[test]
fn test_async_read_reports_missing_typed_fixture_evidence() {
    use crate::qubit_fs_testkit::ContractCheckOutcome;

    for (gap, id) in [
        (AsyncReadEvidenceGap::Preparation, ContractCheckId::ReadBasic),
        (
            AsyncReadEvidenceGap::CurrentVersion,
            ContractCheckId::ReadIfMatchCurrent,
        ),
        (AsyncReadEvidenceGap::StaleVersion, ContractCheckId::ReadIfMatchStale),
    ] {
        let fixture = AsyncReadEvidenceGapFixture {
            inner: AsyncMemoryFixture::with_all_capabilities(),
            gap,
        };
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(run.failures().is_empty(), "{id}: {:?}", run.failures());
            assert!(matches!(
                run.report().checks()[0].outcome(),
                ContractCheckOutcome::Unverified { .. }
            ));
            assert!(run.cleanup().completed(), "{id} cleanup must complete");
        });
        assert!(fixture.inner.is_empty(), "{id} must clean up the seeded file");
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_fixture_defaults_return_explicit_unsupported_evidence() {
    use crate::qubit_fs_testkit::CopyScenario;
    use crate::qubit_fs_testkit::DeleteScenario;
    use crate::qubit_fs_testkit::ReadScenario;
    use crate::qubit_fs_testkit::WriteScenario;

    let fixture = DefaultAsyncFixture::new();
    let path = fixture.path("default-hook").expect("valid fixture path");
    run_controlled(async {
        assert!(!fixture.copy_fallback_only());
        assert_eq!(fixture.list_prefix(&path, "child").expect("prefix"), "child");
        assert!(matches!(
            fixture.snapshot_namespace_paths().await,
            Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.seed_file("file", b"bytes").await,
            Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.read_file(&path).await,
            Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.exists_out_of_band(&path).await,
            Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.write_file_out_of_band(&path, b"bytes").await,
            Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.resource_version(&path).await,
            Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.stale_resource_version(&path).await,
            Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.checksum_failure_case("file").await,
            Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.seed_empty_directory("directory").await,
            Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.seed_symlink("link").await,
            Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.copy_fast_path_case(CopyMethod::ServerSide).await,
            Ok(crate::qubit_fs_testkit::FixtureSupport::Unsupported)
        ));
        for scenario in [
            CopyScenario::Basic,
            CopyScenario::AtomicFile,
            CopyScenario::DurableFile,
            CopyScenario::AtomicTree,
            CopyScenario::DurableTree,
            CopyScenario::ServerSide,
            CopyScenario::Conflict,
        ] {
            assert!(matches!(
                fixture.prepare_copy(scenario, "source", "target", b"bytes").await,
                Ok(crate::qubit_fs_testkit::FixturePreparation::Unavailable { .. })
            ));
        }
        assert!(matches!(
            fixture.prepare_delete(DeleteScenario::Basic, "file", b"bytes").await,
            Ok(crate::qubit_fs_testkit::FixturePreparation::Unavailable { .. })
        ));
        for scenario in [
            ReadScenario::Basic,
            ReadScenario::Range,
            ReadScenario::RangeLimit,
            ReadScenario::IfMatchCurrent,
            ReadScenario::IfMatchStale,
            ReadScenario::IfNoneMatchCurrent,
            ReadScenario::IfNoneMatchStale,
            ReadScenario::Checksum,
            ReadScenario::ChecksumCorruption,
        ] {
            assert!(matches!(
                fixture.prepare_read(scenario, "file", b"bytes").await,
                Ok(crate::qubit_fs_testkit::FixturePreparation::Unavailable { .. })
            ));
        }
        for scenario in [
            WriteScenario::CreateConflict,
            WriteScenario::Replace,
            WriteScenario::Append,
            WriteScenario::IfAbsent,
            WriteScenario::IfMatch,
            WriteScenario::AtomicReplace,
        ] {
            assert!(matches!(
                fixture.prepare_write(scenario, "file", b"bytes").await,
                Ok(crate::qubit_fs_testkit::FixturePreparation::Unavailable { .. })
            ));
        }
        for scenario in [WriteScenario::Create, WriteScenario::Abort, WriteScenario::Durable] {
            assert!(matches!(
                fixture.prepare_write(scenario, "file", b"bytes").await,
                Ok(crate::qubit_fs_testkit::FixturePreparation::Ready(_))
            ));
        }
    });
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_copy_checks_reject_unadvertised_guarantees() {
    for id in [
        ContractCheckId::CopyServerSide,
        ContractCheckId::CopyAtomicFile,
        ContractCheckId::CopyDurableFile,
    ] {
        let fixture = AsyncMemoryFixture::new();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(run.requirements_satisfied(), "{id}: {:?}", run.failures());
        });
        assert!(fixture.is_empty(), "{id}: fixture leaked resources");
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_server_side_copy_reports_unavailable_fixture_evidence() {
    let fixture = AsyncMemoryFixture::with_conditional_case_unavailable(
        crate::common::UnavailableScenario::Capability(FileSystemCapability::ServerSideCopy),
    );
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::CopyServerSide).await;
        assert!(!run.was_interrupted());
        assert!(run.failures().is_empty());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            crate::qubit_fs_testkit::ContractCheckOutcome::Unverified { .. }
        ));
        assert!(run.cleanup().completed());
    });
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_server_side_copy_rejects_an_unadvertised_capability() {
    let fixture = AsyncMemoryFixture::without_optional_capabilities();
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::CopyServerSide).await;
        assert!(!run.was_interrupted());
        assert!(run.failures().is_empty());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            crate::qubit_fs_testkit::ContractCheckOutcome::RejectedAsExpected
        ));
    });
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_temp_directory_paths_retain_creation_and_cleanup_failures() {
    for (fault, id) in [
        (
            crate::common::AsyncMemoryFault::TempCreationFails,
            ContractCheckId::TempDirectory,
        ),
        (
            crate::common::AsyncMemoryFault::TempCleanupFailsOnce,
            ContractCheckId::TempDirectory,
        ),
        (
            crate::common::AsyncMemoryFault::TempKeepFails,
            ContractCheckId::TempRepeatedLifecycle,
        ),
    ] {
        let fixture = AsyncMemoryFixture::with_fault(fault);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(!run.failures().is_empty(), "{fault:?} must fail {id}");
            assert!(run.failures().iter().all(|failure| { failure.check() == Some(id) }));
        });
        assert!(fixture.is_empty(), "{fault:?} must clean up the fixture");
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_expected_write_rejection_cleanup_errors_are_retained() {
    let fault = crate::common::AsyncMemoryFault::ConditionalAbortReportsPublished;
    let id = ContractCheckId::WriteIfAbsent;
    let fixture = AsyncMemoryFixture::with_fault(fault);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id).await;
        assert!(
            run.failures().iter().any(|failure| failure.check() == Some(id)),
            "{fault:?} must be attributed to {id}"
        );
    });
    assert!(fixture.is_empty(), "{fault:?} must clean up the fixture");
}

#[cfg(feature = "async")]
#[test]
fn test_async_temp_contracts_retain_indexed_fixture_failures() {
    use crate::common::shared_model::FixtureHook;

    for id in [
        ContractCheckId::TempFile,
        ContractCheckId::TempDirectory,
        ContractCheckId::TempAtomic,
        ContractCheckId::TempRepeatedLifecycle,
    ] {
        let baseline = AsyncMemoryFixture::with_all_capabilities();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&baseline);
            let run = suite.run_check(id).await;
            assert!(run.requirements_satisfied(), "baseline {id}: {:?}", run.failures());
        });
        let counts = (
            baseline.path_call_count(),
            baseline.exists_call_count(),
            baseline.seed_file_call_count(),
            baseline.seed_directory_call_count(),
            baseline.temp_creation_call_count(),
            baseline.fixture_hook_call_count(FixtureHook::ReadFile),
            baseline.fixture_hook_call_count(FixtureHook::WriteFile),
        );
        assert!(baseline.is_empty(), "baseline {id} must clean up");

        let mut cases = Vec::new();
        cases.extend((1..=counts.0).map(AsyncMemoryFixture::with_path_error_call));
        cases.extend((1..=counts.1).map(AsyncMemoryFixture::with_exists_error_call));
        cases.extend((1..=counts.2).map(|call| AsyncMemoryFixture::with_seed_error_calls(Some(call), None)));
        cases.extend((1..=counts.3).map(|call| AsyncMemoryFixture::with_seed_error_calls(None, Some(call))));
        cases.extend((1..=counts.4).map(AsyncMemoryFixture::with_temp_creation_error_call));
        cases.extend(
            (1..=counts.5).map(|call| AsyncMemoryFixture::with_fixture_hook_error(FixtureHook::ReadFile, call)),
        );
        cases.extend(
            (1..=counts.6).map(|call| AsyncMemoryFixture::with_fixture_hook_error(FixtureHook::WriteFile, call)),
        );

        for fixture in cases {
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert!(
                    run.failures().iter().any(|failure| failure.check() == Some(id)),
                    "an indexed fixture failure must be attributed to {id}"
                );
                assert!(
                    run.cleanup().completed(),
                    "indexed fixture failure in {id} must clean up"
                );
            });
            assert!(fixture.is_empty(), "indexed fixture failure in {id} leaked resources");
        }
    }
}

#[cfg(feature = "async")]
#[test]
fn test_async_read_contracts_retain_indexed_fixture_failures() {
    use crate::common::shared_model::FixtureHook;

    let hooks = [
        FixtureHook::ReadFile,
        FixtureHook::ResourceVersion,
        FixtureHook::StaleResourceVersion,
        FixtureHook::ChecksumFailureCase,
    ];
    for id in [
        ContractCheckId::ReadBasic,
        ContractCheckId::ReadChecksum,
        ContractCheckId::ReadChecksumCorruption,
        ContractCheckId::ReadIfMatchCurrent,
        ContractCheckId::ReadIfMatchStale,
        ContractCheckId::ReadIfNoneMatchCurrent,
        ContractCheckId::ReadIfNoneMatchStale,
        ContractCheckId::ReadRange,
        ContractCheckId::ReadRangeLimit,
    ] {
        let baseline = AsyncMemoryFixture::with_all_capabilities();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&baseline);
            let run = suite.run_check(id).await;
            assert!(run.requirements_satisfied(), "baseline {id}: {:?}", run.failures());
        });
        let path_calls = baseline.path_call_count();
        let seed_calls = baseline.seed_file_call_count();
        let hook_calls = hooks.map(|hook| (hook, baseline.fixture_hook_call_count(hook)));
        assert!(baseline.is_empty(), "baseline {id} must clean up");

        let mut cases = Vec::new();
        cases.extend((1..=path_calls).map(AsyncMemoryFixture::with_path_error_call));
        cases.extend((1..=seed_calls).map(|call| AsyncMemoryFixture::with_seed_error_calls(Some(call), None)));
        for (hook, calls) in hook_calls {
            cases.extend((1..=calls).map(|call| AsyncMemoryFixture::with_fixture_hook_error(hook, call)));
        }

        for fixture in cases {
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert!(
                    run.failures().iter().any(|failure| failure.check() == Some(id)),
                    "an indexed fixture failure must be attributed to {id}"
                );
                assert!(
                    run.cleanup().completed(),
                    "indexed fixture failure in {id} must clean up"
                );
            });
            assert!(fixture.is_empty(), "indexed fixture failure in {id} leaked resources");
        }
    }
}

#[cfg(feature = "async")]
#[test]
fn test_async_recursive_delete_contract_runs_with_all_capabilities() {
    let fixture = AsyncMemoryFixture::with_all_capabilities();
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::DeleteTree).await;
        assert!(run.requirements_satisfied(), "{:?}", run.failures());
        assert!(!suite.report().checks().is_empty());
    });
    assert!(fixture.is_empty(), "recursive deletion must leave no fixture resources");
}

#[cfg(feature = "async")]
#[test]
fn test_async_temp_directory_does_not_require_directory_creation_capability() {
    let fixture = AsyncMemoryFixture::temp_only();
    assert!(
        !fixture
            .file_system()
            .properties()
            .capabilities()
            .supports(FileSystemCapability::CreateDirectory)
    );
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempDirectory).await;
        assert!(run.requirements_satisfied(), "{:?}", run.failures());
    });
    assert!(fixture.is_empty());
}

#[cfg(feature = "async")]
#[test]
fn test_async_recursive_delete_reports_missing_final_observation() {
    use crate::qubit_fs_testkit::ContractCheckOutcome;

    for (unsupported_after, directories_supported) in [(4, true), (0, true), (4, false)] {
        let fixture = AsyncDeleteTreeObservationGapFixture {
            inner: AsyncMemoryFixture::with_all_capabilities(),
            observations: AtomicUsize::new(0),
            unsupported_after,
            directories_supported,
        };
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::DeleteTree).await;
            assert!(run.failures().is_empty(), "{:?}", run.failures());
            assert!(matches!(
                run.report().checks()[0].outcome(),
                ContractCheckOutcome::Unverified { .. }
            ));
            assert!(run.cleanup().completed());
        });
        assert!(fixture.inner.is_empty(), "recursive-delete resources must be reclaimed");
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_copy_cancellation_attributes_admission_path_errors() {
    let fixture = AsyncMemoryFixture::new().with_invalid_probe_path("source");
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::AsyncCopyCancelReader).await;
        assert!(!run.failures().is_empty());
        assert!(
            run.failures()
                .iter()
                .any(|failure| { failure.check() == Some(ContractCheckId::AsyncCopyCancelReader) })
        );
        assert!(
            run.failures()
                .iter()
                .any(|failure| { failure.message().contains("copy cancellation request admission failed") })
        );
    });
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_copy_cancellation_retains_probe_failures() {
    for fault in [
        crate::common::AsyncMemoryFault::CopyCancelAcknowledgeFails,
        crate::common::AsyncMemoryFault::CopyCancelDisarmFails,
        crate::common::AsyncMemoryFault::CopyCancelFailsBeforeStage,
    ] {
        let fixture = AsyncMemoryFixture::with_fault(fault);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::AsyncCopyCancelNativeAttempt).await;
            assert!(!run.failures().is_empty(), "{fault:?} must fail the probe");
            assert!(
                run.failures()
                    .iter()
                    .any(|failure| { failure.check() == Some(ContractCheckId::AsyncCopyCancelNativeAttempt) })
            );
            assert!(run.failures().iter().all(|failure| { !failure.message().is_empty() }));
        });
    }

    for (label, fixture) in [
        ("exists", AsyncMemoryFixture::with_cancellation_exists_error_call(1)),
        (
            "read",
            AsyncMemoryFixture::with_cancellation_fixture_hook_error(
                crate::common::shared_model::FixtureHook::ReadFile,
                1,
            ),
        ),
    ] {
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::AsyncCopyCancelNativeAttempt).await;
            assert!(!run.failures().is_empty(), "{label}: {:?}", run.report().checks());
            assert!(
                run.failures()
                    .iter()
                    .any(|failure| { failure.check() == Some(ContractCheckId::AsyncCopyCancelNativeAttempt) })
            );
        });
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_optional_profile_preserves_path_failures() {
    for id in ContractCheckId::ALL.iter().copied() {
        let baseline = AsyncMemoryFixture::without_optional_capabilities();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&baseline);
            let _ = suite.run_check(id).await;
        });
        let path_calls = baseline.path_call_count();
        if path_calls == 0 {
            continue;
        }
        for call in 1..=path_calls {
            let fixture = AsyncMemoryFixture::without_optional_capabilities_with_path_error_call(call);
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert!(!run.failures().is_empty(), "{id}, path {call}");
                assert!(
                    run.failures().iter().all(|failure| failure.check() == Some(id)),
                    "{id}, path {call}"
                );
            });
            assert!(fixture.is_empty(), "{id}, path {call}: fixture leaked resources");
        }
    }
}
