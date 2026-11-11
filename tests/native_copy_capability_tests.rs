// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

pub use ::qubit_fs_testkit;
mod common;

#[cfg(feature = "async")]
use common::AsyncMemoryFixture;
use common::MemoryFixture;
#[cfg(feature = "async")]
use common::async_memory_file_system::run_controlled;
#[cfg(feature = "async")]
use common::native_copy_fixture::AsyncNativeCopyFixture;
use common::native_copy_fixture::NativeCopyFixture;
use qubit_fs::copy::CopyConflictPolicy;
use qubit_fs::metadata::FileSystemCapability;
#[cfg(feature = "async")]
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContractSuite;

fn outcome(fixture: &NativeCopyFixture, id: ContractCheckId) -> ContractCheckOutcome {
    let mut suite = FileSystemContractSuite::new(fixture);
    let run = suite.run_check(id);
    let result = run
        .report()
        .checks()
        .iter()
        .find(|c| c.id() == id)
        .expect("selected check recorded")
        .outcome()
        .clone();
    assert!(run.cleanup().completed(), "cleanup must complete");
    assert!(fixture.is_empty(), "fixture storage must be empty after cleanup");
    result
}

#[test]
fn test_native_copy_basic_requires_copy_but_not_read_or_write() {
    for (read, write) in [(false, false), (false, true), (true, false), (true, true)] {
        let fixture = NativeCopyFixture::new(read, write);
        let result = outcome(&fixture, ContractCheckId::CopyBasic);
        assert_eq!(result, ContractCheckOutcome::Passed, "read={read}, write={write}");
        assert_eq!(fixture.native_calls(), 1);
        assert_eq!(fixture.reader_writer_calls(), (0, 0));
        let snapshot = fixture.cleanup_snapshot();
        let (source, target) = fixture.last_paths();
        assert_eq!(snapshot.get(&source).map(Vec::as_slice), Some(b"copy bytes".as_slice()));
        assert_eq!(snapshot.get(&target).map(Vec::as_slice), Some(b"copy bytes".as_slice()));
        let caps = fixture.file_system().properties().capabilities();
        assert_eq!(caps.supports(FileSystemCapability::Read), read);
        assert_eq!(caps.supports(FileSystemCapability::Write), write);
    }
}

#[test]
fn test_native_copy_conflict_policies_use_native_copy_without_read_or_write() {
    let fixture = NativeCopyFixture::new(false, false);
    let result = outcome(&fixture, ContractCheckId::CopyFallbackOverwriteRejected);
    assert_eq!(result, ContractCheckOutcome::Passed);
    assert_eq!(fixture.native_calls(), 3);
    assert_eq!(fixture.reader_writer_calls(), (0, 0));
    assert_eq!(
        fixture.policies(),
        [
            CopyConflictPolicy::Fail,
            CopyConflictPolicy::Skip,
            CopyConflictPolicy::Overwrite
        ]
    );
    let snapshot = fixture.cleanup_snapshot();
    let (source, target) = fixture.last_paths();
    assert_eq!(snapshot.get(&source).map(Vec::as_slice), Some(b"copy bytes".as_slice()));
    assert_eq!(snapshot.get(&target).map(Vec::as_slice), Some(b"copy bytes".as_slice()));
}

#[test]
fn test_native_copy_error_is_reported_by_selected_check() {
    let fixture = NativeCopyFixture::new(false, false);
    fixture.inject_copy_error();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::CopyBasic);
    assert!(matches!(
        run.report()
            .checks()
            .iter()
            .find(|c| c.id() == ContractCheckId::CopyBasic)
            .map(|c| c.outcome()),
        Some(ContractCheckOutcome::Failed { .. })
    ));
    assert!(run.cleanup().completed());
    assert!(fixture.is_empty());
    let snapshot = fixture.cleanup_snapshot();
    let (source, target) = fixture.last_paths();
    assert_eq!(snapshot.get(&source).map(Vec::as_slice), Some(b"copy bytes".as_slice()));
    assert!(!snapshot.contains_key(&target));
}

#[test]
fn test_native_copy_wrong_published_contents_fail_independent_evidence() {
    let fixture = NativeCopyFixture::new(false, false);
    fixture.inject_wrong_content();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::CopyBasic);
    assert!(matches!(
        run.report()
            .checks()
            .iter()
            .find(|c| c.id() == ContractCheckId::CopyBasic)
            .map(|c| c.outcome()),
        Some(ContractCheckOutcome::Failed { .. })
    ));
    assert!(run.cleanup().completed());
    assert!(fixture.is_empty());
    let snapshot = fixture.cleanup_snapshot();
    let (source, target) = fixture.last_paths();
    assert_eq!(snapshot.get(&source).map(Vec::as_slice), Some(b"copy bytes".as_slice()));
    assert_eq!(
        snapshot.get(&target).map(Vec::as_slice),
        Some(b"wrong native payload".as_slice())
    );
}

#[test]
fn test_copy_without_copy_capability_keeps_expected_rejection() {
    let fixture = MemoryFixture::without_operation_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::CopyBasic);
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::RejectedAsExpected
    ));
    assert!(run.failures().is_empty());
    assert!(run.cleanup().completed());
    assert!(fixture.is_empty());
}

#[test]
fn test_fallback_only_copy_still_rejects_overwrite() {
    let fixture = MemoryFixture::fallback_only();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::CopyFallbackOverwriteRejected);
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::RejectedAsExpected
    ));
    assert!(run.failures().is_empty());
    assert!(run.cleanup().completed());
    assert!(fixture.is_empty());
}

#[test]
fn test_missing_copy_seed_or_observation_stays_unverified() {
    for id in [
        ContractCheckId::CopyBasic,
        ContractCheckId::CopyFallbackOverwriteRejected,
    ] {
        let seed_unavailable = NativeCopyFixture::with_fixture_support(false, false, false, true);
        let mut suite = FileSystemContractSuite::new(&seed_unavailable);
        let run = suite.run_check(id);
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { .. }
        ));
        assert!(run.failures().is_empty());
        assert_eq!(seed_unavailable.native_calls(), 0);
        assert!(run.cleanup().completed());
        assert!(seed_unavailable.is_empty());
        assert!(seed_unavailable.cleanup_snapshot().is_empty());

        let observation_unavailable = NativeCopyFixture::with_fixture_support(false, false, true, false);
        let mut suite = FileSystemContractSuite::new(&observation_unavailable);
        let run = suite.run_check(id);
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { .. }
        ));
        assert!(run.failures().is_empty());
        assert_eq!(observation_unavailable.native_calls(), 0);
        assert!(run.cleanup().completed());
        assert!(observation_unavailable.is_empty());
        let snapshot = observation_unavailable.cleanup_snapshot();
        let (source, target) = observation_unavailable.last_paths();
        assert_eq!(snapshot.get(&source).map(Vec::as_slice), Some(b"copy bytes".as_slice()));
        if id == ContractCheckId::CopyFallbackOverwriteRejected {
            assert_eq!(snapshot.get(&target).map(Vec::as_slice), Some(b"existing".as_slice()));
        } else {
            assert!(!snapshot.contains_key(&target));
        }
    }
}

#[cfg(feature = "async")]
fn async_outcome(fixture: &AsyncNativeCopyFixture, id: ContractCheckId) -> ContractCheckOutcome {
    let mut suite = AsyncFileSystemContractSuite::new(fixture);
    let run = run_controlled(suite.run_check(id));
    let result = run
        .report()
        .checks()
        .iter()
        .find(|c| c.id() == id)
        .expect("selected check recorded")
        .outcome()
        .clone();
    assert!(run.cleanup().completed(), "async cleanup must complete");
    assert!(fixture.is_empty(), "async fixture storage must be empty after cleanup");
    result
}

#[cfg(feature = "async")]
fn assert_async_copy_snapshot(fixture: &AsyncNativeCopyFixture) {
    let snapshot = fixture.cleanup_snapshot();
    let (source, target) = fixture.last_paths();
    assert_eq!(snapshot.get(&source).map(Vec::as_slice), Some(b"copy bytes".as_slice()));
    assert_eq!(snapshot.get(&target).map(Vec::as_slice), Some(b"copy bytes".as_slice()));
}

#[cfg(feature = "async")]
#[test]
fn test_async_native_copy_basic_requires_copy_but_not_read_or_write() {
    for (read, write) in [(false, false), (false, true), (true, false), (true, true)] {
        let fixture = AsyncNativeCopyFixture::new(read, write);
        assert_eq!(
            async_outcome(&fixture, ContractCheckId::CopyBasic),
            ContractCheckOutcome::Passed,
            "read={read}, write={write}"
        );
        assert_eq!(fixture.native_calls(), 1);
        assert_eq!(fixture.reader_writer_calls(), (0, 0));
        assert_async_copy_snapshot(&fixture);
    }
}

#[cfg(feature = "async")]
#[test]
fn test_async_native_copy_conflict_policies_use_native_copy_without_read_or_write() {
    let fixture = AsyncNativeCopyFixture::new(false, false);
    assert_eq!(
        async_outcome(&fixture, ContractCheckId::CopyFallbackOverwriteRejected),
        ContractCheckOutcome::Passed
    );
    assert_eq!(fixture.native_calls(), 3);
    assert_eq!(fixture.reader_writer_calls(), (0, 0));
    assert_eq!(
        fixture.policies(),
        [
            CopyConflictPolicy::Fail,
            CopyConflictPolicy::Skip,
            CopyConflictPolicy::Overwrite
        ]
    );
    assert_async_copy_snapshot(&fixture);
}

#[cfg(feature = "async")]
#[test]
fn test_async_native_copy_repeated_execute_preserves_completed_operation() {
    let fixture = AsyncNativeCopyFixture::new(false, false);
    assert_eq!(
        async_outcome(&fixture, ContractCheckId::CopyRepeatedExecute),
        ContractCheckOutcome::Passed
    );
    assert_eq!(fixture.native_calls(), 1);
    assert_eq!(fixture.reader_writer_calls(), (0, 0));
    assert_async_copy_snapshot(&fixture);
}

#[cfg(feature = "async")]
#[test]
fn test_async_native_copy_errors_and_wrong_content_fail_selected_check() {
    for wrong_content in [false, true] {
        let fixture = AsyncNativeCopyFixture::new(false, false);
        if wrong_content {
            fixture.inject_wrong_content();
        } else {
            fixture.inject_copy_error();
        }
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = run_controlled(suite.run_check(ContractCheckId::CopyBasic));
        assert!(matches!(
            run.report()
                .checks()
                .iter()
                .find(|c| c.id() == ContractCheckId::CopyBasic)
                .map(|c| c.outcome()),
            Some(ContractCheckOutcome::Failed { .. })
        ));
        assert!(run.cleanup().completed());
        assert!(fixture.is_empty());
        let snapshot = fixture.cleanup_snapshot();
        let (source, target) = fixture.last_paths();
        assert_eq!(snapshot.get(&source).map(Vec::as_slice), Some(b"copy bytes".as_slice()));
        if wrong_content {
            assert_eq!(
                snapshot.get(&target).map(Vec::as_slice),
                Some(b"wrong native payload".as_slice())
            );
        } else {
            assert!(!snapshot.contains_key(&target));
        }
    }
}

#[cfg(feature = "async")]
#[test]
fn test_async_copy_without_copy_capability_keeps_expected_rejection() {
    let fixture = AsyncMemoryFixture::without_core_capabilities();
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let run = run_controlled(suite.run_check(ContractCheckId::CopyBasic));
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::RejectedAsExpected
    ));
    assert!(run.failures().is_empty());
    assert!(run.cleanup().completed());
    assert!(fixture.is_empty());
}

#[cfg(feature = "async")]
#[test]
fn test_async_fallback_only_copy_still_rejects_overwrite() {
    let fixture = AsyncMemoryFixture::fallback_only();
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let run = run_controlled(suite.run_check(ContractCheckId::CopyFallbackOverwriteRejected));
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::RejectedAsExpected
    ));
    assert!(run.failures().is_empty());
    assert!(run.cleanup().completed());
    assert!(fixture.is_empty());
}

#[cfg(feature = "async")]
#[test]
fn test_async_missing_copy_seed_or_observation_stays_unverified() {
    for id in [
        ContractCheckId::CopyBasic,
        ContractCheckId::CopyFallbackOverwriteRejected,
    ] {
        let seed_unavailable = AsyncNativeCopyFixture::with_fixture_support(false, false, false, true);
        let mut suite = AsyncFileSystemContractSuite::new(&seed_unavailable);
        let run = run_controlled(suite.run_check(id));
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { .. }
        ));
        assert!(run.failures().is_empty());
        assert_eq!(seed_unavailable.native_calls(), 0);
        assert!(run.cleanup().completed());
        assert!(seed_unavailable.is_empty());
        assert!(seed_unavailable.cleanup_snapshot().is_empty());

        let observation_unavailable = AsyncNativeCopyFixture::with_fixture_support(false, false, true, false);
        let mut suite = AsyncFileSystemContractSuite::new(&observation_unavailable);
        let run = run_controlled(suite.run_check(id));
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { .. }
        ));
        assert!(run.failures().is_empty());
        assert_eq!(observation_unavailable.native_calls(), 0);
        assert!(run.cleanup().completed());
        assert!(observation_unavailable.is_empty());
        let snapshot = observation_unavailable.cleanup_snapshot();
        let (source, target) = observation_unavailable.last_paths();
        assert_eq!(snapshot.get(&source).map(Vec::as_slice), Some(b"copy bytes".as_slice()));
        if id == ContractCheckId::CopyFallbackOverwriteRejected {
            assert_eq!(snapshot.get(&target).map(Vec::as_slice), Some(b"existing".as_slice()));
        } else {
            assert!(!snapshot.contains_key(&target));
        }
    }
}
