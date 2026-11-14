// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use std::cell::Cell;

pub use ::qubit_fs_testkit;
use qubit_fs::error::FsErrorKind;
use qubit_fs::metadata::AtomicityRequirement;
use qubit_fs::path::Path;
use qubit_fs::path::RelativePath;
use qubit_fs::temp::PersistFailureState;
use qubit_fs::temp::PersistOptions;
use qubit_fs::temp::TempOptions;
use qubit_fs::temp::TempResourceState;
use qubit_fs::write::WriteOptions;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContractSuite;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureSupport;

use self::common::MemoryFault;
use self::common::MemoryFixture;

fn assert_fixture_file(fixture: &MemoryFixture, path: &Path, expected: &[u8]) {
    match fixture.read_file(path).expect("fixture file observation must succeed") {
        FixtureSupport::Supported(actual) => assert_eq!(actual, expected),
        FixtureSupport::Unsupported => panic!("fixture must observe the published temporary file"),
    }
}

struct TempFilePostPublicationReadUnsupportedFixture {
    inner: MemoryFixture,
    read_calls: Cell<usize>,
}

impl TempFilePostPublicationReadUnsupportedFixture {
    fn new() -> Self {
        Self {
            inner: MemoryFixture::with_all_capabilities(),
            read_calls: Cell::new(0),
        }
    }
}

impl FileSystemFixture for TempFilePostPublicationReadUnsupportedFixture {
    fn file_system(&self) -> &qubit_fs::FileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn read_file(&self, path: &Path) -> qubit_fs_testkit::FixtureResult<FixtureSupport<Vec<u8>>> {
        let call = self.read_calls.get() + 1;
        self.read_calls.set(call);
        if call == 4 {
            Ok(FixtureSupport::Unsupported)
        } else {
            self.inner.read_file(path)
        }
    }

    fn exists_out_of_band(&self, path: &Path) -> qubit_fs_testkit::FixtureResult<FixtureSupport<bool>> {
        self.inner.exists_out_of_band(path)
    }

    fn write_file_out_of_band(&self, path: &Path, bytes: &[u8]) -> qubit_fs_testkit::FixtureResult<FixtureSupport<()>> {
        self.inner.write_file_out_of_band(path, bytes)
    }

    fn seed_empty_directory(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<FixtureSupport<Path>> {
        self.inner.seed_empty_directory(relative)
    }

    fn teardown(&self) -> qubit_fs_testkit::FixtureResult<()> {
        self.inner.teardown()
    }
}

struct TempExistenceMismatchFixture {
    inner: MemoryFixture,
    mismatch_at: usize,
    error_at: Option<usize>,
    exists_calls: Cell<usize>,
}

impl TempExistenceMismatchFixture {
    fn new(inner: MemoryFixture, mismatch_at: usize) -> Self {
        Self {
            inner,
            mismatch_at,
            error_at: None,
            exists_calls: Cell::new(0),
        }
    }

    fn failing_observation(inner: MemoryFixture, error_at: usize) -> Self {
        Self {
            inner,
            mismatch_at: 0,
            error_at: Some(error_at),
            exists_calls: Cell::new(0),
        }
    }
}

impl FileSystemFixture for TempExistenceMismatchFixture {
    fn file_system(&self) -> &qubit_fs::FileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn read_file(&self, path: &Path) -> qubit_fs_testkit::FixtureResult<FixtureSupport<Vec<u8>>> {
        self.inner.read_file(path)
    }

    fn exists_out_of_band(&self, path: &Path) -> qubit_fs_testkit::FixtureResult<FixtureSupport<bool>> {
        let call = self.exists_calls.get() + 1;
        self.exists_calls.set(call);
        if self.error_at == Some(call) {
            return Err(qubit_fs_testkit::FixtureError::new(
                "injected independent existence observation failure",
            ));
        }
        let observed = self.inner.exists_out_of_band(path)?;
        if call == self.mismatch_at {
            Ok(match observed {
                FixtureSupport::Supported(exists) => FixtureSupport::Supported(!exists),
                FixtureSupport::Unsupported => FixtureSupport::Supported(false),
            })
        } else {
            Ok(observed)
        }
    }

    fn write_file_out_of_band(&self, path: &Path, bytes: &[u8]) -> qubit_fs_testkit::FixtureResult<FixtureSupport<()>> {
        self.inner.write_file_out_of_band(path, bytes)
    }

    fn seed_empty_directory(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<FixtureSupport<Path>> {
        self.inner.seed_empty_directory(relative)
    }

    fn teardown(&self) -> qubit_fs_testkit::FixtureResult<()> {
        self.inner.teardown()
    }
}

#[test]
fn test_sync_temp_contracts_attribute_each_existence_observation_error() {
    for id in [
        ContractCheckId::TempFile,
        ContractCheckId::TempDirectory,
        ContractCheckId::TempAtomic,
        ContractCheckId::TempRepeatedLifecycle,
    ] {
        let baseline_fixture = if id == ContractCheckId::TempAtomic {
            MemoryFixture::without_atomic_temp_persist()
        } else {
            MemoryFixture::with_all_capabilities()
        };
        let mut baseline_suite = FileSystemContractSuite::new(&baseline_fixture);
        let baseline = baseline_suite.run_check(id);
        let observation_calls = baseline_fixture.exists_call_count();
        assert!(
            baseline.requirements_satisfied(),
            "baseline {id}: {:?}",
            baseline.failures()
        );
        assert!(observation_calls > 0);
        assert!(baseline.cleanup().completed());
        assert!(baseline_fixture.is_empty());

        for error_at in 1..=observation_calls {
            let inner = if id == ContractCheckId::TempAtomic {
                MemoryFixture::without_atomic_temp_persist()
            } else {
                MemoryFixture::with_all_capabilities()
            };
            let fixture = TempExistenceMismatchFixture::failing_observation(inner, error_at);
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id);
            assert!(!run.requirements_satisfied(), "{id}, observation {error_at}");
            assert!(run.failures().iter().any(|failure| failure.check() == Some(id)));
            assert!(run.cleanup().completed(), "{id}, observation {error_at}");
            assert!(fixture.inner.is_empty(), "{id}, observation {error_at}");
        }
    }
}

#[test]
fn test_sync_temp_keep_transfers_identity_and_payload() {
    let fixture = MemoryFixture::new();
    let file_system = fixture.file_system();
    let mut temporary = file_system
        .create_temp_file(TempOptions::default())
        .expect("temporary file should open");
    let source = temporary.path().clone();
    assert_eq!(TempResourceState::Owned, temporary.state());

    file_system
        .write_all(&source, b"kept-payload", WriteOptions::default())
        .expect("temporary source should accept fixture payload");
    let outcome = temporary.keep().expect("keep should publish the source");

    assert_eq!(TempResourceState::Kept, temporary.state());
    assert_ne!(&source, outcome.target(), "keep must publish a distinct identity");
    assert!(!file_system.exists(&source).expect("source observation must succeed"));
    assert!(
        file_system
            .exists(outcome.target())
            .expect("kept target observation must succeed")
    );
    assert_fixture_file(&fixture, outcome.target(), b"kept-payload");
    assert_eq!(outcome.target(), temporary.path());
    assert!(temporary.cleanup().is_err(), "kept handle must not reclaim the target");
    assert!(temporary.keep().is_err(), "kept handle must reject a second keep");

    fixture.teardown().expect("fixture teardown must succeed");
    assert!(fixture.is_empty(), "teardown must release the kept target");
}

#[test]
fn test_sync_temp_failed_atomic_persist_has_indeterminate_source() {
    let fixture = MemoryFixture::with_fault(MemoryFault::AtomicTempPersistNonAtomic);
    let file_system = fixture.file_system();
    let mut temporary = file_system
        .create_temp_file(TempOptions::default())
        .expect("temporary file should open");
    let source = temporary.path().clone();
    file_system
        .write_all(&source, b"retry-payload", WriteOptions::default())
        .expect("temporary source should accept fixture payload");
    let target = Path::parse("/contract/retry-persist-target").expect("target should parse");

    let failure = temporary
        .persist(
            &target,
            PersistOptions::default().with_atomicity(AtomicityRequirement::Required),
        )
        .expect_err("non-atomic publication must fail a required persist");
    assert_eq!(PersistFailureState::PublishedSourceIndeterminate, failure.state());
    assert_eq!(TempResourceState::Indeterminate, temporary.state());
    assert!(file_system.exists(&target).expect("target observation must succeed"));
    assert_fixture_file(&fixture, &target, b"retry-payload");

    assert_eq!(
        FsErrorKind::InvalidState,
        temporary
            .cleanup()
            .expect_err("indeterminate source must not be cleaned")
            .kind()
    );
    assert_eq!(TempResourceState::Indeterminate, temporary.state());

    fixture.teardown().expect("fixture teardown must succeed");
    assert!(fixture.is_empty(), "teardown must release the published target");
}

#[test]
fn test_sync_temp_contract_suite_preserves_published_contents() {
    for id in [
        ContractCheckId::TempFile,
        ContractCheckId::TempDirectory,
        ContractCheckId::TempAtomic,
        ContractCheckId::TempRepeatedLifecycle,
    ] {
        let fixture = MemoryFixture::new();
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        assert!(run.requirements_satisfied(), "{id}: {:?}", run.failures());
    }
}

#[test]
fn test_sync_temp_contract_suite_rejects_lost_file_and_directory_contents() {
    for (fault, id) in [
        (MemoryFault::TempKeepDropsFileContents, ContractCheckId::TempFile),
        (MemoryFault::TempPersistDropsFileContents, ContractCheckId::TempFile),
        (MemoryFault::TempPersistDropsFileContents, ContractCheckId::TempAtomic),
        (
            MemoryFault::TempKeepDropsDirectoryChildren,
            ContractCheckId::TempDirectory,
        ),
        (
            MemoryFault::TempPersistDropsDirectoryChildren,
            ContractCheckId::TempDirectory,
        ),
        (
            MemoryFault::TempOverwriteDropsDirectoryChildren,
            ContractCheckId::TempDirectory,
        ),
    ] {
        let fixture = MemoryFixture::with_fault(fault);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        assert!(!run.requirements_satisfied(), "{fault:?} unexpectedly passed {id}");
        assert!(
            run.failures().iter().any(|failure| failure.check() == Some(id)),
            "{fault:?}: {id}"
        );
    }
}

#[test]
fn test_sync_temp_directory_overwrite_accepts_native_empty_target() {
    let fixture = MemoryFixture::with_fault(MemoryFault::TempNativeDirectoryOverwrite);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::TempDirectory);
    assert!(run.requirements_satisfied(), "{:?}", run.failures());
    fixture.teardown().expect("fixture teardown must succeed");
    assert!(fixture.is_empty());
}

#[test]
fn test_sync_temp_name_faults_fail_the_matching_check() {
    for fault in [
        MemoryFault::TempWrongPrefixBasename,
        MemoryFault::TempPrefixOnlyInParent,
    ] {
        for id in [ContractCheckId::TempFile, ContractCheckId::TempDirectory] {
            let fixture = MemoryFixture::with_fault(fault);
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id);
            assert!(!run.requirements_satisfied(), "{fault:?} unexpectedly passed {id}");
            assert!(
                run.failures().iter().any(|failure| failure.check() == Some(id)),
                "{fault:?} must fail {id}: {:?}",
                run.failures()
            );
            assert!(run.failures().iter().all(|failure| failure.check() == Some(id)));
            fixture.teardown().expect("fixture teardown must succeed");
            assert!(fixture.is_empty(), "{fault:?} must clean up after {id}");
        }
    }
}

#[test]
fn test_sync_temp_lifecycle_rejects_reused_live_paths() {
    let fixture = MemoryFixture::with_fault(MemoryFault::TempReusesLivePath);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::TempRepeatedLifecycle);
    assert!(!run.requirements_satisfied(), "reused live paths must fail");
    assert!(
        run.failures()
            .iter()
            .any(|failure| { failure.check() == Some(ContractCheckId::TempRepeatedLifecycle) })
    );
    fixture.teardown().expect("fixture teardown must succeed");
    assert!(fixture.is_empty());
}

#[test]
fn test_sync_temp_file_and_directory_lifecycles_keep_interleaved_resources_isolated() {
    let fixture = MemoryFixture::new();
    let file_system = fixture.file_system();
    let mut file_a = file_system
        .create_temp_file(TempOptions::default())
        .expect("file A should open");
    let mut file_b = file_system
        .create_temp_file(TempOptions::default())
        .expect("file B should open");
    let file_b_path = file_b.path().clone();
    file_system
        .write_all(&file_b_path, b"file B sentinel", WriteOptions::default())
        .expect("file B should accept sentinel");
    file_a.cleanup().expect("file A cleanup should succeed");
    let mut file_c = file_system
        .create_temp_file(TempOptions::default())
        .expect("file C should open");
    assert_ne!(&file_b_path, file_c.path());
    assert_fixture_file(&fixture, &file_b_path, b"file B sentinel");

    let mut directory_a = file_system
        .create_temp_directory(TempOptions::default())
        .expect("directory A should open");
    let mut directory_b = file_system
        .create_temp_directory(TempOptions::default())
        .expect("directory B should open");
    let directory_b_path = directory_b.path().clone();
    let nested_sentinel = directory_b_path.join(&RelativePath::parse("nested/sentinel").expect("valid child"));
    let sentinel_write = fixture
        .write_file_out_of_band(&nested_sentinel, b"directory B sentinel")
        .expect("nested sentinel write should succeed");
    assert!(
        matches!(sentinel_write, FixtureSupport::Supported(())),
        "fixture must write the nested directory sentinel",
    );
    directory_a.cleanup().expect("directory A cleanup should succeed");
    let mut directory_c = file_system
        .create_temp_directory(TempOptions::default())
        .expect("directory C should open");
    assert_ne!(&directory_b_path, directory_c.path());
    assert_fixture_file(&fixture, &nested_sentinel, b"directory B sentinel");

    file_b.cleanup().expect("file B cleanup should succeed");
    file_c.cleanup().expect("file C cleanup should succeed");
    directory_b.cleanup().expect("directory B cleanup should succeed");
    directory_c.cleanup().expect("directory C cleanup should succeed");
    fixture.teardown().expect("fixture teardown must succeed");
    assert!(fixture.is_empty());
}

#[test]
fn test_sync_temp_content_evidence_is_unverified_when_hooks_are_unsupported() {
    for fault in [
        MemoryFault::TempWriteHookUnsupported,
        MemoryFault::TempReadHookUnsupported,
    ] {
        let fixture = MemoryFixture::with_fault(fault);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempFile);
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { reason } if reason.contains("hook")
        ));
        assert!(!run.requirements_satisfied());
    }
}

#[test]
fn test_sync_temp_directory_content_evidence_is_unverified_when_hooks_are_unsupported() {
    for fault in [
        MemoryFault::TempWriteHookUnsupported,
        MemoryFault::TempReadHookUnsupported,
    ] {
        let fixture = MemoryFixture::with_fault(fault);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempDirectory);

        assert!(run.failures().is_empty(), "{fault:?}: {:?}", run.failures());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { reason } if reason.contains("fixture")
        ));
        assert!(
            !run.requirements_satisfied(),
            "missing independent directory evidence must not count as verified: {fault:?}"
        );
        assert!(fixture.is_empty(), "{fault:?}: directory resources must be cleaned");
    }
}

#[test]
fn test_sync_temp_directory_independent_parent_seed_gaps_are_failures_and_cleaned() {
    let cases = [
        (
            "unsupported parent seed",
            MemoryFixture::with_unavailable_seed_calls(Some(1), None),
        ),
        (
            "parent seed setup error",
            MemoryFixture::with_seed_error_calls(None, Some(1)),
        ),
    ];

    for (case, fixture) in cases {
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempDirectory);

        assert!(!run.requirements_satisfied(), "{case} must not pass");
        assert_eq!(run.failures().len(), 1, "{case}: {:?}", run.failures());
        assert_eq!(
            run.failures()[0].check(),
            Some(ContractCheckId::TempDirectory),
            "{case}"
        );
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Failed { .. }
        ));
        assert!(fixture.is_empty(), "{case}: fixture resources must be cleaned");
    }
}

#[test]
fn test_sync_temp_directory_content_setup_error_remains_a_failure() {
    for hook in [
        self::common::shared_model::FixtureHook::WriteFile,
        self::common::shared_model::FixtureHook::ReadFile,
    ] {
        let fixture = MemoryFixture::with_fixture_hook_error(hook, 1);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempDirectory);

        assert!(!run.requirements_satisfied(), "{hook:?}");
        assert_eq!(run.failures().len(), 1, "{hook:?}: {:?}", run.failures());
        assert_eq!(
            run.failures()[0].check(),
            Some(ContractCheckId::TempDirectory),
            "{hook:?}"
        );
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Failed { .. }
        ));
        assert!(
            fixture.is_empty(),
            "{hook:?}: content setup failure must clean temporary resources"
        );
    }
}

#[test]
fn test_sync_temp_content_setup_errors_remain_failures() {
    let fixture = MemoryFixture::with_fixture_hook_error_and_limits(
        self::common::shared_model::FixtureHook::WriteFile,
        1,
        qubit_fs::metadata::FileSystemLimits::unknown(),
    );
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::TempFile);
    assert!(!run.requirements_satisfied());
    assert!(
        run.failures()
            .iter()
            .any(|failure| failure.check() == Some(ContractCheckId::TempFile))
    );
}

#[test]
fn test_sync_temp_content_setup_does_not_require_facade_write_capability() {
    let fixture = MemoryFixture::temp_only();
    assert!(
        !fixture
            .file_system()
            .properties()
            .capabilities()
            .supports(qubit_fs::metadata::FileSystemCapability::Write)
    );
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::TempFile);
    assert!(run.requirements_satisfied(), "{:?}", run.failures());
}

#[test]
fn test_sync_temp_file_independent_parent_seed_gaps_are_failures_and_cleaned() {
    let cases = [
        (
            "unsupported parent seed",
            MemoryFixture::with_unavailable_seed_calls(Some(1), None),
        ),
        (
            "parent seed setup error",
            MemoryFixture::with_seed_error_calls(None, Some(1)),
        ),
    ];

    for (case, fixture) in cases {
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempFile);

        assert!(!run.requirements_satisfied(), "{case} must not pass");
        assert_eq!(run.failures().len(), 1, "{case}: {:?}", run.failures());
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::TempFile), "{case}");
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Failed { .. }
        ));
        assert!(run.cleanup().completed(), "{case}: fixture teardown must complete");
        assert!(fixture.is_empty(), "{case}: fixture resources must be cleaned");
    }
}

#[test]
fn test_sync_temp_file_publication_records_unsupported_read_evidence() {
    let fixture = TempFilePostPublicationReadUnsupportedFixture::new();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::TempFile);

    assert_eq!(
        fixture.read_calls.get(),
        4,
        "unsupported evidence must occur after publication"
    );
    assert!(
        run.failures().is_empty(),
        "unexpected fixture failures: {:?}",
        run.failures()
    );
    assert!(run.report().checks().iter().any(|check| {
        check.id() == ContractCheckId::TempFile
            && matches!(
                check.outcome(),
                ContractCheckOutcome::Unverified { reason } if reason.contains("after publication")
            )
    }));
    assert!(
        !run.requirements_satisfied(),
        "missing publication evidence must remain unverified"
    );
    assert!(run.cleanup().completed(), "temp-file cleanup must complete");
    assert!(
        fixture.inner.is_empty(),
        "temp-file run must clean the underlying provider"
    );
}

#[test]
fn test_sync_temp_contracts_reject_each_contradictory_existence_observation() {
    for id in [
        ContractCheckId::TempFile,
        ContractCheckId::TempDirectory,
        ContractCheckId::TempAtomic,
        ContractCheckId::TempRepeatedLifecycle,
    ] {
        let baseline_fixture = if id == ContractCheckId::TempAtomic {
            MemoryFixture::without_atomic_temp_persist()
        } else {
            MemoryFixture::with_all_capabilities()
        };
        let mut baseline_suite = FileSystemContractSuite::new(&baseline_fixture);
        let baseline = baseline_suite.run_check(id);
        assert!(
            baseline.requirements_satisfied(),
            "baseline {id}: {:?}",
            baseline.failures()
        );
        let observation_calls = baseline_fixture.exists_call_count();
        assert!(observation_calls > 0, "{id} must use independent existence evidence");
        assert!(baseline.cleanup().completed(), "baseline {id} cleanup must complete");
        assert!(baseline_fixture.is_empty(), "baseline {id} leaked resources");

        for mismatch_at in 1..=observation_calls {
            let inner = if id == ContractCheckId::TempAtomic {
                MemoryFixture::without_atomic_temp_persist()
            } else {
                MemoryFixture::with_all_capabilities()
            };
            let fixture = TempExistenceMismatchFixture::new(inner, mismatch_at);
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id);

            assert!(!run.requirements_satisfied(), "{id}, observation {mismatch_at}");
            assert!(
                run.failures().iter().any(|failure| failure.check() == Some(id)),
                "{id}, observation {mismatch_at}: {:?}",
                run.failures()
            );
            assert!(matches!(
                run.report().checks()[0].outcome(),
                ContractCheckOutcome::Failed { .. }
            ));
            assert!(
                run.cleanup().completed(),
                "{id}, observation {mismatch_at}: cleanup must complete"
            );
            assert!(
                fixture.inner.is_empty(),
                "{id}, observation {mismatch_at}: fixture resources must be cleaned"
            );
        }
    }
}
