// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

#![cfg(feature = "async")]

pub use ::qubit_fs_testkit;
mod common;
use qubit_fs::metadata::AtomicityRequirement;
use qubit_fs::path::Path;
use qubit_fs::temp::PersistFailureState;
use qubit_fs::temp::PersistOptions;
use qubit_fs::temp::TempOptions;
use qubit_fs::temp::TempResourceState;
use qubit_fs::write::WriteOptions;
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::FixtureFuture;
use qubit_fs_testkit::FixtureSupport;

use self::common::AsyncMemoryFault;
use self::common::AsyncMemoryFixture;
use self::common::async_memory_file_system::run_controlled;

struct AsyncTempHookFixture {
    inner: AsyncMemoryFixture,
    unsupported_write: bool,
    unsupported_read: bool,
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
