// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================

mod common;

use common::MemoryFault;
use common::MemoryFixture;
use qubit_fs::error::FsErrorKind;
use qubit_fs::metadata::AtomicityRequirement;
use qubit_fs::path::Path;
use qubit_fs::temp::PersistFailureState;
use qubit_fs::temp::PersistOptions;
use qubit_fs::temp::TempOptions;
use qubit_fs::temp::TempResourceState;
use qubit_fs::write::WriteOptions;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureSupport;

fn assert_fixture_file(fixture: &MemoryFixture, path: &Path, expected: &[u8]) {
    match fixture.read_file(path).expect("fixture file observation must succeed") {
        FixtureSupport::Supported(actual) => assert_eq!(actual, expected),
        FixtureSupport::Unsupported => panic!("fixture must observe the published temporary file"),
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

    let teardown = fixture.teardown().expect("fixture teardown must succeed");
    assert!(matches!(teardown, FixtureSupport::Supported(())));
    assert!(fixture.is_empty(), "teardown must release the kept target");
}

#[test]
fn test_sync_temp_failed_atomic_persist_remains_cleanup_recoverable() {
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
    assert_eq!(PersistFailureState::PublishedSourceRetained, failure.state());
    assert_eq!(TempResourceState::CleanupRequired, temporary.state());
    assert!(file_system.exists(&target).expect("target observation must succeed"));
    assert_fixture_file(&fixture, &target, b"retry-payload");

    temporary
        .cleanup()
        .expect("failed persist must leave cleanup available");
    assert_eq!(TempResourceState::Cleaned, temporary.state());
    assert!(temporary.cleanup().is_err(), "cleanup must not run twice");
    assert_eq!(
        FsErrorKind::InvalidState,
        temporary
            .cleanup()
            .expect_err("cleaned handle must reject another cleanup")
            .kind()
    );

    let teardown = fixture.teardown().expect("fixture teardown must succeed");
    assert!(matches!(teardown, FixtureSupport::Supported(())));
    assert!(fixture.is_empty(), "teardown must release the published target");
}
