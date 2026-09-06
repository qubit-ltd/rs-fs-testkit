// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================

#![cfg(feature = "async")]

mod common;

use common::AsyncMemoryFault;
use common::AsyncMemoryFixture;
use common::async_memory_file_system::run_controlled;
use qubit_fs::metadata::AtomicityRequirement;
use qubit_fs::path::Path;
use qubit_fs::temp::PersistFailureState;
use qubit_fs::temp::PersistOptions;
use qubit_fs::temp::TempOptions;
use qubit_fs::temp::TempResourceState;
use qubit_fs::write::WriteOptions;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::FixtureSupport;

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

    run_controlled(file_system.write_all(&source, b"kept-payload", WriteOptions::default()))
        .expect("temporary source should accept fixture payload");
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

    let teardown = run_controlled(fixture.teardown()).expect("fixture teardown must succeed");
    assert!(matches!(teardown, FixtureSupport::Supported(())));
    assert!(fixture.is_empty(), "teardown must release the kept target");
}

#[test]
fn test_async_temp_failed_atomic_persist_remains_cleanup_recoverable() {
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::AtomicTempPersistNonAtomic);
    let file_system = fixture.file_system();
    let mut temporary =
        run_controlled(file_system.create_temp_file(TempOptions::default())).expect("temporary file should open");
    let source = temporary.path().clone();
    run_controlled(file_system.write_all(&source, b"retry-payload", WriteOptions::default()))
        .expect("temporary source should accept fixture payload");
    let target = Path::parse("/contract/async-retry-persist-target").expect("target should parse");

    let failure = run_controlled(temporary.persist(
        &target,
        PersistOptions::default().with_atomicity(AtomicityRequirement::Required),
    ))
    .expect_err("non-atomic publication must fail a required persist");
    assert_eq!(PersistFailureState::PublishedSourceRetained, failure.state());
    assert_eq!(TempResourceState::CleanupRequired, temporary.state());
    assert!(run_controlled(file_system.exists(&target)).expect("target observation must succeed"));
    assert_fixture_file(&fixture, &target, b"retry-payload");

    run_controlled(temporary.cleanup()).expect("failed persist must leave cleanup available");
    assert_eq!(TempResourceState::Cleaned, temporary.state());
    assert!(
        run_controlled(temporary.cleanup()).is_err(),
        "cleanup must not run twice"
    );

    let teardown = run_controlled(fixture.teardown()).expect("fixture teardown must succeed");
    assert!(matches!(teardown, FixtureSupport::Supported(())));
    assert!(fixture.is_empty(), "teardown must release the published target");
}
