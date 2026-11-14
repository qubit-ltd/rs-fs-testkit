// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

#![cfg(feature = "async")]

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

pub use ::qubit_fs_testkit;
mod common;
use qubit_fs::metadata::AtomicityRequirement;
use qubit_fs::path::Path;
use qubit_fs::path::RelativePath;
use qubit_fs::temp::PersistFailureState;
use qubit_fs::temp::PersistOptions;
use qubit_fs::temp::TempOptions;
use qubit_fs::temp::TempResourceState;
use qubit_fs::write::WriteOptions;
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureFuture;
use qubit_fs_testkit::FixtureSupport;

use self::common::AsyncMemoryFault;
use self::common::AsyncMemoryFixture;
use self::common::async_memory_file_system::run_controlled;

struct AsyncTempHookFixture {
    inner: AsyncMemoryFixture,
    unsupported_write: bool,
    unsupported_read: bool,
    unsupported_exists: bool,
    unsupported_nested_exists: bool,
}

struct AsyncTempExistenceMismatchFixture {
    inner: AsyncMemoryFixture,
    mismatch_at: usize,
    error_at: Option<usize>,
    exists_calls: AtomicUsize,
}

impl AsyncTempExistenceMismatchFixture {
    fn new(inner: AsyncMemoryFixture, mismatch_at: usize) -> Self {
        Self {
            inner,
            mismatch_at,
            error_at: None,
            exists_calls: AtomicUsize::new(0),
        }
    }

    fn with_error(inner: AsyncMemoryFixture, error_at: usize) -> Self {
        Self {
            inner,
            mismatch_at: 0,
            error_at: Some(error_at),
            exists_calls: AtomicUsize::new(0),
        }
    }
}

impl AsyncFileSystemFixture for AsyncTempExistenceMismatchFixture {
    fn file_system(&self) -> &qubit_fs::AsyncFileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn read_file<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<Vec<u8>>> {
        self.inner.read_file(path)
    }

    fn exists_out_of_band<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<bool>> {
        let call = self.exists_calls.fetch_add(1, Ordering::Relaxed) + 1;
        if self.error_at == Some(call) {
            return Box::pin(async { Err(FixtureError::new("injected existence observation failure")) });
        }
        let observed = self.inner.exists_out_of_band(path);
        Box::pin(async move {
            let observed = observed.await?;
            if call == self.mismatch_at {
                Ok(match observed {
                    FixtureSupport::Supported(exists) => FixtureSupport::Supported(!exists),
                    FixtureSupport::Unsupported => FixtureSupport::Supported(false),
                })
            } else {
                Ok(observed)
            }
        })
    }

    fn write_file_out_of_band<'a>(&'a self, path: &'a Path, bytes: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<()>> {
        self.inner.write_file_out_of_band(path, bytes)
    }

    fn seed_empty_directory<'a>(&'a self, relative: &'a str) -> FixtureFuture<'a, FixtureSupport<Path>> {
        self.inner.seed_empty_directory(relative)
    }

    fn teardown(&self) -> FixtureFuture<'_, ()> {
        self.inner.teardown()
    }
}

impl AsyncFileSystemFixture for AsyncTempHookFixture {
    fn file_system(&self) -> &qubit_fs::AsyncFileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn read_file<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<Vec<u8>>> {
        if self.unsupported_read {
            Box::pin(async { Ok(FixtureSupport::Unsupported) })
        } else {
            self.inner.read_file(path)
        }
    }

    fn exists_out_of_band<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<bool>> {
        if self.unsupported_exists || (self.unsupported_nested_exists && path.as_str().ends_with("/nested/sentinel")) {
            Box::pin(async { Ok(FixtureSupport::Unsupported) })
        } else {
            self.inner.exists_out_of_band(path)
        }
    }

    fn write_file_out_of_band<'a>(&'a self, path: &'a Path, bytes: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<()>> {
        if self.unsupported_write {
            Box::pin(async { Ok(FixtureSupport::Unsupported) })
        } else {
            self.inner.write_file_out_of_band(path, bytes)
        }
    }

    fn teardown(&self) -> FixtureFuture<'_, ()> {
        self.inner.teardown()
    }
}

fn assert_fixture_file(fixture: &AsyncMemoryFixture, path: &Path, expected: &[u8]) {
    match run_controlled(fixture.read_file(path)).expect("fixture file observation must succeed") {
        FixtureSupport::Supported(actual) => assert_eq!(actual, expected),
        FixtureSupport::Unsupported => panic!("fixture must observe the published temporary file"),
    }
}

#[test]
fn test_async_temp_keep_transfers_identity_and_payload() {
    let fixture = AsyncMemoryFixture::new();
    let file_system = fixture.file_system();
    let mut temporary =
        run_controlled(file_system.create_temp_file(TempOptions::default())).expect("temporary file should open");
    let source = temporary.path().clone();
    assert_eq!(TempResourceState::Owned, temporary.state());

    let mut operation = file_system
        .begin_write_all(source.clone(), b"kept-payload".to_vec(), WriteOptions::default())
        .expect("temporary source write preflight should succeed");
    run_controlled(operation.execute()).expect("temporary source should accept fixture payload");
    let outcome = run_controlled(temporary.keep()).expect("keep should publish the source");

    assert_eq!(TempResourceState::Kept, temporary.state());
    assert_ne!(&source, outcome.target(), "keep must publish a distinct identity");
    assert!(!run_controlled(file_system.exists(&source)).expect("source observation must succeed"));
    assert!(run_controlled(file_system.exists(outcome.target())).expect("kept target observation must succeed"));
    assert_fixture_file(&fixture, outcome.target(), b"kept-payload");
    assert_eq!(outcome.target(), temporary.path());
    assert!(
        run_controlled(temporary.cleanup()).is_err(),
        "kept handle must not reclaim the target"
    );
    assert!(
        run_controlled(temporary.keep()).is_err(),
        "kept handle must reject a second keep"
    );

    run_controlled(fixture.teardown()).expect("fixture teardown must succeed");
    assert!(fixture.is_empty(), "teardown must release the kept target");
}

#[test]
fn test_async_temp_failed_atomic_persist_has_indeterminate_source() {
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::AtomicTempPersistNonAtomic);
    let file_system = fixture.file_system();
    let mut temporary =
        run_controlled(file_system.create_temp_file(TempOptions::default())).expect("temporary file should open");
    let source = temporary.path().clone();
    let mut operation = file_system
        .begin_write_all(source.clone(), b"retry-payload".to_vec(), WriteOptions::default())
        .expect("temporary source write preflight should succeed");
    run_controlled(operation.execute()).expect("temporary source should accept fixture payload");
    let target = Path::parse("/contract/async-retry-persist-target").expect("target should parse");

    let failure = run_controlled(temporary.persist(
        &target,
        PersistOptions::default().with_atomicity(AtomicityRequirement::Required),
    ))
    .expect_err("non-atomic publication must fail a required persist");
    assert_eq!(PersistFailureState::PublishedSourceIndeterminate, failure.state());
    assert_eq!(TempResourceState::Indeterminate, temporary.state());
    assert!(run_controlled(file_system.exists(&target)).expect("target observation must succeed"));
    assert_fixture_file(&fixture, &target, b"retry-payload");

    assert!(
        run_controlled(temporary.cleanup()).is_err(),
        "indeterminate source must not be cleaned"
    );
    assert_eq!(TempResourceState::Indeterminate, temporary.state());

    run_controlled(fixture.teardown()).expect("fixture teardown must succeed");
    assert!(fixture.is_empty(), "teardown must release the published target");
}

#[test]
fn test_async_temp_publication_contract_rejects_lost_file_contents_and_directory_children() {
    for (fault, id) in [
        (AsyncMemoryFault::TempKeepDropsFileContents, ContractCheckId::TempFile),
        (
            AsyncMemoryFault::TempPersistDropsFileContents,
            ContractCheckId::TempFile,
        ),
        (
            AsyncMemoryFault::TempPersistDropsFileContents,
            ContractCheckId::TempAtomic,
        ),
        (
            AsyncMemoryFault::TempKeepDropsDirectoryChildren,
            ContractCheckId::TempDirectory,
        ),
        (
            AsyncMemoryFault::TempPersistDropsDirectoryChildren,
            ContractCheckId::TempDirectory,
        ),
        (
            AsyncMemoryFault::TempPersistDropsDirectoryChildren,
            ContractCheckId::TempAtomic,
        ),
        (
            AsyncMemoryFault::TempOverwriteDropsDirectoryChildren,
            ContractCheckId::TempDirectory,
        ),
    ] {
        let fixture = AsyncMemoryFixture::with_fault(fault);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(!run.requirements_satisfied(), "{fault:?} must fail {id}");
            assert!(run.failures().iter().any(|failure| failure.check() == Some(id)));
        });
        assert!(fixture.is_empty(), "{fault:?} must clean up after {id}");
    }
}

#[test]
fn test_async_temp_directory_overwrite_accepts_native_empty_target() {
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::TempNativeDirectoryOverwrite);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempDirectory).await;
        assert!(run.requirements_satisfied(), "{:?}", run.failures());
    });
    run_controlled(fixture.teardown()).expect("fixture teardown must succeed");
    assert!(fixture.is_empty());
}

#[test]
fn test_async_temp_name_faults_fail_the_matching_check() {
    for fault in [
        AsyncMemoryFault::TempWrongPrefixBasename,
        AsyncMemoryFault::TempPrefixOnlyInParent,
    ] {
        for id in [ContractCheckId::TempFile, ContractCheckId::TempDirectory] {
            let fixture = AsyncMemoryFixture::with_fault(fault);
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert!(!run.requirements_satisfied(), "{fault:?} unexpectedly passed {id}");
                assert!(
                    run.failures().iter().any(|failure| failure.check() == Some(id)),
                    "{fault:?} must fail {id}: {:?}",
                    run.failures()
                );
                assert!(run.failures().iter().all(|failure| failure.check() == Some(id)));
            });
            run_controlled(fixture.teardown()).expect("fixture teardown must succeed");
            assert!(fixture.is_empty(), "{fault:?} must clean up after {id}");
        }
    }
}

#[test]
fn test_async_temp_lifecycle_rejects_reused_live_paths() {
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::TempReusesLivePath);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempRepeatedLifecycle).await;
        assert!(!run.requirements_satisfied(), "reused live paths must fail");
        assert!(
            run.failures()
                .iter()
                .any(|failure| { failure.check() == Some(ContractCheckId::TempRepeatedLifecycle) })
        );
    });
    run_controlled(fixture.teardown()).expect("fixture teardown must succeed");
    assert!(fixture.is_empty());
}

#[test]
fn test_async_temp_file_and_directory_lifecycles_keep_interleaved_resources_isolated() {
    let fixture = AsyncMemoryFixture::new();
    let file_system = fixture.file_system();
    let (mut file_a, mut file_b) = run_controlled(async {
        let file_a = file_system
            .create_temp_file(TempOptions::default())
            .await
            .expect("file A should open");
        let file_b = file_system
            .create_temp_file(TempOptions::default())
            .await
            .expect("file B should open");
        (file_a, file_b)
    });
    let file_b_path = file_b.path().clone();
    let mut write = file_system
        .begin_write_all(
            file_b_path.clone(),
            b"file B sentinel".to_vec(),
            WriteOptions::default(),
        )
        .expect("file B write preflight should succeed");
    run_controlled(write.execute()).expect("file B should accept sentinel");
    run_controlled(file_a.cleanup()).expect("file A cleanup should succeed");
    let mut file_c = run_controlled(file_system.create_temp_file(TempOptions::default())).expect("file C should open");
    assert_ne!(&file_b_path, file_c.path());
    assert_fixture_file(&fixture, &file_b_path, b"file B sentinel");

    let (mut directory_a, mut directory_b) = run_controlled(async {
        let directory_a = file_system
            .create_temp_directory(TempOptions::default())
            .await
            .expect("directory A should open");
        let directory_b = file_system
            .create_temp_directory(TempOptions::default())
            .await
            .expect("directory B should open");
        (directory_a, directory_b)
    });
    let directory_b_path = directory_b.path().clone();
    let nested_sentinel = directory_b_path.join(&RelativePath::parse("nested/sentinel").expect("valid child"));
    let sentinel_write = run_controlled(fixture.write_file_out_of_band(&nested_sentinel, b"directory B sentinel"))
        .expect("nested sentinel write should succeed");
    assert!(
        matches!(sentinel_write, FixtureSupport::Supported(())),
        "fixture must write the nested directory sentinel",
    );
    run_controlled(directory_a.cleanup()).expect("directory A cleanup should succeed");
    let mut directory_c =
        run_controlled(file_system.create_temp_directory(TempOptions::default())).expect("directory C should open");
    assert_ne!(&directory_b_path, directory_c.path());
    assert_fixture_file(&fixture, &nested_sentinel, b"directory B sentinel");

    run_controlled(file_b.cleanup()).expect("file B cleanup should succeed");
    run_controlled(file_c.cleanup()).expect("file C cleanup should succeed");
    run_controlled(directory_b.cleanup()).expect("directory B cleanup should succeed");
    run_controlled(directory_c.cleanup()).expect("directory C cleanup should succeed");
    run_controlled(fixture.teardown()).expect("fixture teardown must succeed");
    assert!(fixture.is_empty());
}

#[test]
fn test_async_temp_only_capability_does_not_require_facade_write() {
    let fixture = AsyncMemoryFixture::temp_only();
    assert!(
        !fixture
            .file_system()
            .properties()
            .capabilities()
            .supports(qubit_fs::metadata::FileSystemCapability::Write)
    );
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempFile).await;
        assert!(run.requirements_satisfied(), "{:?}", run.failures());
    });
    assert!(fixture.is_empty());
}

#[test]
fn test_async_temp_missing_content_hooks_are_unverified_with_specific_reasons() {
    for (unsupported_write, unsupported_read, hook_name) in
        [(true, false, "write_file_out_of_band"), (false, true, "read_file")]
    {
        let fixture = AsyncTempHookFixture {
            inner: AsyncMemoryFixture::new(),
            unsupported_write,
            unsupported_read,
            unsupported_exists: false,
            unsupported_nested_exists: false,
        };
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::TempRepeatedLifecycle).await;
            assert!(run.failures().is_empty(), "{:?}", run.failures());
            assert!(matches!(
                run.report()
                    .checks()
                    .iter()
                    .find(|check| check.id() == ContractCheckId::TempRepeatedLifecycle)
                    .expect("temporary lifecycle check must be reported")
                    .outcome(),
                qubit_fs_testkit::ContractCheckOutcome::Unverified { reason } if reason.contains(hook_name)
            ));
        });
        assert!(fixture.inner.is_empty());
    }
}

#[test]
fn test_async_temp_file_unavailable_parent_seed_is_a_failure_and_tears_down() {
    let fixture = AsyncMemoryFixture::with_unavailable_seed_calls(Some(1), None);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempFile).await;
        assert!(
            !run.requirements_satisfied(),
            "missing parent seed must fail the basic temp-file check"
        );
        assert!(
            run.failures()
                .iter()
                .any(|failure| failure.check() == Some(ContractCheckId::TempFile))
        );
    });
    run_controlled(fixture.teardown()).expect("fixture teardown must succeed");
    assert!(
        fixture.is_empty(),
        "unsupported parent preparation must not leak fixture resources"
    );
}

#[test]
fn test_async_temp_file_unavailable_cleanup_observation_fails_and_tears_down() {
    let fixture = AsyncTempHookFixture {
        inner: AsyncMemoryFixture::new(),
        unsupported_write: false,
        unsupported_read: false,
        unsupported_exists: true,
        unsupported_nested_exists: false,
    };
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempFile).await;
        assert!(
            !run.requirements_satisfied(),
            "missing cleanup evidence must fail the basic temp-file check"
        );
        assert!(
            run.failures()
                .iter()
                .any(|failure| failure.check() == Some(ContractCheckId::TempFile))
        );
    });
    run_controlled(fixture.inner.teardown()).expect("fixture teardown must succeed");
    assert!(
        fixture.inner.is_empty(),
        "unsupported cleanup observation must not leak resources"
    );
}

#[test]
fn test_async_temp_directory_repeated_lifecycle_unavailable_sentinel_observation_is_unverified() {
    let fixture = AsyncTempHookFixture {
        inner: AsyncMemoryFixture::new(),
        unsupported_write: false,
        unsupported_read: false,
        unsupported_exists: false,
        unsupported_nested_exists: true,
    };
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempRepeatedLifecycle).await;
        assert!(
            run.failures().is_empty(),
            "unsupported observation is an evidence limit: {:?}",
            run.failures()
        );
        assert!(matches!(
            run.report()
                .checks()
                .iter()
                .find(|check| check.id() == ContractCheckId::TempRepeatedLifecycle)
                .expect("temporary lifecycle check must be reported")
                .outcome(),
            qubit_fs_testkit::ContractCheckOutcome::Unverified { reason }
                if reason.contains("exists_out_of_band")
        ));
    });
    run_controlled(fixture.inner.teardown()).expect("fixture teardown must succeed");
    assert!(
        fixture.inner.is_empty(),
        "unsupported observation path must still be reclaimed"
    );
}

#[test]
fn test_async_temp_contracts_reject_each_contradictory_existence_observation() {
    for id in [
        ContractCheckId::TempFile,
        ContractCheckId::TempDirectory,
        ContractCheckId::TempAtomic,
        ContractCheckId::TempRepeatedLifecycle,
    ] {
        let baseline_fixture = if id == ContractCheckId::TempAtomic {
            AsyncMemoryFixture::without_atomic_temp_persist()
        } else {
            AsyncMemoryFixture::new()
        };
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&baseline_fixture);
            let baseline = suite.run_check(id).await;
            assert!(
                baseline.requirements_satisfied(),
                "baseline {id}: {:?}",
                baseline.failures()
            );
            assert!(baseline.cleanup().completed(), "baseline {id} cleanup must complete");
        });
        let observation_calls = baseline_fixture.exists_call_count();
        assert!(observation_calls > 0, "{id} must use independent existence evidence");
        assert!(baseline_fixture.is_empty(), "baseline {id} leaked resources");

        for mismatch_at in 1..=observation_calls {
            let inner = if id == ContractCheckId::TempAtomic {
                AsyncMemoryFixture::without_atomic_temp_persist()
            } else {
                AsyncMemoryFixture::new()
            };
            let fixture = AsyncTempExistenceMismatchFixture::new(inner, mismatch_at);
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert!(
                    !run.requirements_satisfied(),
                    "{id}, observation {mismatch_at} unexpectedly passed"
                );
                assert!(
                    run.failures().iter().any(|failure| failure.check() == Some(id)),
                    "{id}, observation {mismatch_at}: {:?}",
                    run.failures()
                );
                assert!(matches!(
                    run.report()
                        .checks()
                        .iter()
                        .find(|check| check.id() == id)
                        .expect("temporary check must be reported")
                        .outcome(),
                    qubit_fs_testkit::ContractCheckOutcome::Failed { .. }
                ));
                assert!(
                    run.cleanup().completed(),
                    "{id}, observation {mismatch_at}: cleanup must complete"
                );
            });
            assert!(
                fixture.inner.is_empty(),
                "{id}, observation {mismatch_at}: fixture resources must be cleaned"
            );
        }
    }
}

#[test]
fn test_async_temp_contracts_retain_each_existence_observation_error() {
    for id in [
        ContractCheckId::TempFile,
        ContractCheckId::TempDirectory,
        ContractCheckId::TempAtomic,
        ContractCheckId::TempRepeatedLifecycle,
    ] {
        let baseline_fixture = if id == ContractCheckId::TempAtomic {
            AsyncMemoryFixture::without_atomic_temp_persist()
        } else {
            AsyncMemoryFixture::new()
        };
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&baseline_fixture);
            let baseline = suite.run_check(id).await;
            assert!(
                baseline.requirements_satisfied(),
                "baseline {id}: {:?}",
                baseline.failures()
            );
            assert!(baseline.cleanup().completed(), "baseline {id} cleanup must complete");
        });
        let observation_calls = baseline_fixture.exists_call_count();
        assert!(observation_calls > 0, "{id} must use independent existence evidence");
        assert!(baseline_fixture.is_empty(), "baseline {id} leaked resources");

        for error_at in 1..=observation_calls {
            let inner = if id == ContractCheckId::TempAtomic {
                AsyncMemoryFixture::without_atomic_temp_persist()
            } else {
                AsyncMemoryFixture::new()
            };
            let fixture = AsyncTempExistenceMismatchFixture::with_error(inner, error_at);
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert!(
                    !run.requirements_satisfied(),
                    "{id}, observation {error_at} unexpectedly passed"
                );
                assert!(
                    run.failures().iter().any(|failure| failure.check() == Some(id)),
                    "{id}, observation {error_at}: {:?}",
                    run.failures()
                );
                assert!(matches!(
                    run.report()
                        .checks()
                        .iter()
                        .find(|check| check.id() == id)
                        .expect("temporary check must be reported")
                        .outcome(),
                    qubit_fs_testkit::ContractCheckOutcome::Failed { .. }
                ));
                assert!(
                    run.cleanup().completed(),
                    "{id}, observation {error_at}: cleanup must complete"
                );
            });
            assert!(
                fixture.inner.is_empty(),
                "{id}, observation {error_at}: fixture resources must be cleaned"
            );
        }
    }
}

#[test]
fn test_async_temp_content_setup_hook_errors_fail_the_check() {
    for hook in [
        common::shared_model::FixtureHook::WriteFile,
        common::shared_model::FixtureHook::ReadFile,
    ] {
        let fixture = AsyncMemoryFixture::with_fixture_hook_error(hook, 1);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::TempFile).await;
            assert!(!run.requirements_satisfied(), "{hook:?} setup error must fail");
            assert!(!run.failures().is_empty());
            assert!(
                run.failures()
                    .iter()
                    .all(|failure| failure.check() == Some(ContractCheckId::TempFile))
            );
        });
        assert!(fixture.is_empty(), "{hook:?} error requires teardown cleanup");
    }
}
