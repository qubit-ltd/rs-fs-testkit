// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Focused selection must execute only the requested check and its own setup.

use qubit_fs as qfs;
use qubit_fs_testkit as testkit;

mod common;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::FileSystemContractSuite;

use self::common::MemoryFixture;
/// Every synchronous catalog entry can provide evidence in isolation.
#[test]
fn test_focused_sync_all_checks() {
    for id in ContractCheckId::ALL
        .iter()
        .copied()
        .filter(|id| id.supports_synchronous())
    {
        let fixture = MemoryFixture::with_all_capabilities();
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        assert_eq!(run.report().checks().len(), 1, "{id}");
        assert_eq!(run.report().checks()[0].id(), id);
        run.assert_satisfied();
        assert!(fixture.is_empty(), "{id}: cleanup must still run");
    }
}

/// Owning operation, repeat and each cancellation stage are independent
/// entries.
#[cfg(feature = "async")]
#[test]
fn test_focused_async_all_checks() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    run_controlled(async {
        for id in ContractCheckId::ALL.iter().copied() {
            let fixture = AsyncMemoryFixture::new();
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert_eq!(run.report().checks().len(), 1, "{id}");
            assert_eq!(run.report().checks()[0].id(), id);
            run.assert_satisfied();
        }
    });
}

/// A basic-write fault cannot affect a selected replacement check.
#[cfg(feature = "async")]
#[test]
fn test_focused_replacement_does_not_execute_basic_write() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::BasicCommitFails);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::WriteReplace).await;
        run.assert_satisfied();
        assert_eq!(run.report().checks().len(), 1);
    });
}

/// Ordinary metadata mismatches retain their identity without panic conversion.
#[test]
fn test_focused_stat_failure_has_identity() {
    let fixture = MemoryFixture::with_fault(self::common::MemoryFault::WrongStatKind);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::StatFileKind);
    assert!(!run.requirements_satisfied());
    assert_eq!(run.failures().len(), 1);
    assert_eq!(run.failures()[0].check(), Some(ContractCheckId::StatFileKind));
    assert!(run.failures()[0].take_panic_payload().is_none());
}

/// A synchronous request for an async-only check is a failed selection.
#[test]
fn test_focused_sync_rejects_async_only_check() {
    let fixture = MemoryFixture::with_all_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteCancelOpen);
    assert!(!run.requirements_satisfied());
    assert_eq!(run.report().checks().len(), 1);
    assert_eq!(run.failures()[0].check(), Some(ContractCheckId::WriteCancelOpen));
    assert!(fixture.is_empty());
}

/// Finite path limits exercise facade admission and record an actual rejection.
#[test]
fn test_focused_property_admission_has_rejection_evidence() {
    use qubit_fs::metadata::FileSystemLimit;
    use qubit_fs::metadata::FileSystemLimits;
    use qubit_fs_testkit::ContractCheckOutcome;
    for (id, limits) in [
        (
            ContractCheckId::PropertiesLimitPathAdmission,
            FileSystemLimits::unknown().with_max_path_text_bytes(FileSystemLimit::Maximum(4)),
        ),
        (
            ContractCheckId::PropertiesLimitComponentAdmission,
            FileSystemLimits::unknown().with_max_component_text_bytes(FileSystemLimit::Maximum(4)),
        ),
    ] {
        let fixture = MemoryFixture::with_limits(limits);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        run.assert_satisfied_with(&[id]);
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::RejectedAsExpected
        ));
    }
}

/// A fixture path failure belongs only to the path-constraints check.
#[test]
fn test_focused_snapshot_does_not_prepare_fixture_path() {
    use qubit_fs::FileSystem;
    use qubit_fs::path::Path;
    use qubit_fs_testkit::FileSystemFixture;
    use qubit_fs_testkit::FixtureError;
    use qubit_fs_testkit::FixtureResult;

    struct MissingPathFixture(MemoryFixture);
    impl FileSystemFixture for MissingPathFixture {
        fn file_system(&self) -> &FileSystem {
            self.0.file_system()
        }
        fn path(&self, _: &str) -> FixtureResult<Path> {
            Err(FixtureError::new("fixture path unavailable"))
        }
        fn teardown(&self) -> FixtureResult<()> {
            self.0.teardown()
        }
    }
    let fixture = MissingPathFixture(MemoryFixture::with_all_capabilities());
    FileSystemContractSuite::new(&fixture)
        .run_check(ContractCheckId::PropertiesSnapshot)
        .assert_satisfied();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::PropertiesPathConstraints);
    assert!(!run.requirements_satisfied());
    assert_eq!(
        run.failures()[0].check(),
        Some(ContractCheckId::PropertiesPathConstraints)
    );
    assert!(std::error::Error::source(&run.failures()[0]).is_some());
    assert!(run.failures()[0].take_panic_payload().is_none());
}

/// Creating only the leaf must not satisfy recursive directory creation.
#[test]
fn test_recursive_creation_requires_ancestors() {
    let fixture = MemoryFixture::with_fault(self::common::MemoryFault::RecursiveCreateLeavesParentsMissing);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::DirectoryRecursive);
    assert!(!run.requirements_satisfied());
    assert_eq!(run.failures()[0].check(), Some(ContractCheckId::DirectoryRecursive));
    assert!(run.failures()[0].message().contains("ancestor"));
    assert!(fixture.is_empty());
}

/// Asynchronous recursion observes the parent independently of the provider
/// outcome.
#[cfg(feature = "async")]
#[test]
fn test_async_recursive_creation_requires_ancestors() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::RecursiveCreateLeavesParentsMissing);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::DirectoryRecursive).await;
        assert!(!run.requirements_satisfied());
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::DirectoryRecursive));
        assert!(run.failures()[0].message().contains("ancestor"));
        assert!(fixture.is_empty());
    });
}

/// Absent representation capabilities cannot imply a rejection was exercised.
#[test]
fn test_representation_absence_is_not_rejection_evidence() {
    let fixture = MemoryFixture::without_optional_capabilities();
    for id in [
        ContractCheckId::RepresentationEmpty,
        ContractCheckId::RepresentationSymlink,
    ] {
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        run.assert_satisfied();
        assert!(matches!(run.report().checks()[0].outcome(),
            testkit::ContractCheckOutcome::NotApplicable { reason } if !reason.is_empty()));
    }
}

/// Stale conditional deletion is tested explicitly and keeps its check
/// identity.
#[cfg(feature = "async")]
#[test]
fn test_async_stale_delete_is_rejected() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::IgnoreDeleteIfMatch);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::DeleteIfMatch).await;
        assert!(!run.requirements_satisfied());
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::DeleteIfMatch));
        assert!(run.failures()[0].message().contains("stale version was accepted"));
        assert!(run.failures()[0].take_panic_payload().is_none());
    });
}

/// A metadata fault affects the metadata-requesting check, not basic listing.
#[test]
fn test_focused_list_metadata_has_its_own_failure() {
    let fixture = MemoryFixture::with_fault(self::common::MemoryFault::ListDropsMetadata);
    FileSystemContractSuite::new(&fixture)
        .run_check(ContractCheckId::ListBasic)
        .assert_satisfied();
    let fixture = MemoryFixture::with_fault(self::common::MemoryFault::ListDropsMetadata);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::ListPagination);
    assert!(!run.requirements_satisfied());
    assert_eq!(run.failures()[0].check(), Some(ContractCheckId::ListPagination));
    assert!(run.failures()[0].take_panic_payload().is_none());
    assert!(fixture.is_empty());
}

/// An atomicity violation belongs to the selected rename guarantee.
#[cfg(feature = "async")]
#[test]
fn test_focused_async_rename_guarantee_has_identity() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    run_controlled(async {
        let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::AtomicRenameNonAtomic);
        AsyncFileSystemContractSuite::new(&fixture)
            .run_check(ContractCheckId::RenameBasic)
            .await
            .assert_satisfied();
        let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::AtomicRenameNonAtomic);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::RenameAtomic).await;
        assert!(!run.requirements_satisfied());
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::RenameAtomic));
        assert!(run.failures()[0].take_panic_payload().is_none());
        assert!(fixture.is_empty());
    });
}

/// Each copy cancellation probe owns only its selected report entry.
#[cfg(feature = "async")]
#[test]
fn test_focused_copy_cancellation_stages() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    run_controlled(async {
        for id in [
            ContractCheckId::AsyncCopyCancelNativeAttempt,
            ContractCheckId::AsyncCopyCancelReader,
            ContractCheckId::AsyncCopyCancelWriter,
            ContractCheckId::AsyncCopyCancelCommit,
        ] {
            let fixture = AsyncMemoryFixture::new();
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            run.assert_satisfied_with(&[id]);
            assert_eq!(run.report().checks().len(), 1);
            assert_eq!(run.report().checks()[0].id(), id);
            assert!(fixture.is_empty());
        }
    });
}

/// Basic copy has its own setup, outcome and cleanup, without sibling checks.
#[test]
fn test_focused_copy_basic() {
    let fixture = MemoryFixture::with_all_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::CopyBasic);
    assert_eq!(run.report().checks().len(), 1);
    run.assert_satisfied();
    assert!(fixture.is_empty());
}

/// Each owning copy check uses a fresh operation and preserves its own
/// evidence.
#[cfg(feature = "async")]
#[test]
fn test_focused_async_copy_basic_and_repeat() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    run_controlled(async {
        for id in [ContractCheckId::CopyBasic, ContractCheckId::CopyRepeatedExecute] {
            let fixture = AsyncMemoryFixture::new();
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert_eq!(run.report().checks().len(), 1);
            assert_eq!(run.report().checks()[0].id(), id);
            run.assert_satisfied();
        }
    });
}

/// Missing copy dependencies must produce actual rejection evidence.
#[test]
fn test_focused_copy_missing_dependencies() {
    let fixture = MemoryFixture::without_operation_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::CopyBasic);
    run.assert_satisfied();
    assert!(matches!(
        run.report().checks()[0].outcome(),
        testkit::ContractCheckOutcome::RejectedAsExpected
    ));
}

#[cfg(feature = "async")]
#[test]
fn test_focused_async_copy_missing_dependencies() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::without_core_capabilities();
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::CopyBasic).await;
        run.assert_satisfied();
        assert!(matches!(
            run.report().checks()[0].outcome(),
            testkit::ContractCheckOutcome::RejectedAsExpected
        ));
    });
}

/// A basic-copy publication fault cannot fail the independently selected
/// repeat.
#[cfg(feature = "async")]
#[test]
fn test_focused_copy_repeat_does_not_execute_basic() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    run_controlled(async {
        let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::CopyDropsTarget);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::CopyRepeatedExecute).await;
        run.assert_satisfied();
        assert_eq!(run.report().checks().len(), 1);
        let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::CopyDropsTarget);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::CopyBasic).await;
        assert!(!run.requirements_satisfied());
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::CopyBasic));
        assert!(run.failures()[0].take_panic_payload().is_none());
    });
}

/// Conflict checks own their source and target and record only their selected
/// ID.
#[test]
fn test_focused_copy_conflicts() {
    for fixture in [MemoryFixture::with_native_copy(), MemoryFixture::fallback_only()] {
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::CopyFallbackOverwriteRejected);
        assert_eq!(run.report().checks().len(), 1);
        run.assert_satisfied();
        assert!(fixture.is_empty());
    }
}

#[cfg(feature = "async")]
#[test]
fn test_focused_async_copy_conflicts() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    run_controlled(async {
        for fixture in [
            AsyncMemoryFixture::with_native_copy(),
            AsyncMemoryFixture::fallback_only(),
        ] {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::CopyFallbackOverwriteRejected).await;
            assert_eq!(run.report().checks().len(), 1);
            run.assert_satisfied();
        }
    });
}

/// A fixture cannot turn missing advertised copy evidence into a successful
/// skip.
#[test]
fn test_focused_copy_unavailable_conflict_is_unverified() {
    let fixture = MemoryFixture::with_conditional_case_unavailable(crate::common::UnavailableScenario::CopyOverwrite);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::CopyFallbackOverwriteRejected);
    assert!(!run.requirements_satisfied());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        testkit::ContractCheckOutcome::Unverified { .. }
    ));
    assert!(run.failures().is_empty());
    assert!(fixture.is_empty());
}

#[cfg(feature = "async")]
#[test]
fn test_focused_async_copy_unavailable_conflict_is_unverified() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture =
        AsyncMemoryFixture::with_conditional_case_unavailable(crate::common::UnavailableScenario::CopyOverwrite);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::CopyFallbackOverwriteRejected).await;
        assert!(!run.requirements_satisfied());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            testkit::ContractCheckOutcome::Unverified { .. }
        ));
        assert!(run.failures().is_empty());
    });
}

/// Reported overwrite statistics cannot substitute for independent target
/// bytes.
#[test]
fn test_focused_copy_detects_unpublished_overwrite() {
    let fixture = MemoryFixture::with_fault(self::common::MemoryFault::CopyOverwriteKeepsTarget);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::CopyFallbackOverwriteRejected);
    assert!(!run.requirements_satisfied());
    assert_eq!(
        run.failures()[0].check(),
        Some(ContractCheckId::CopyFallbackOverwriteRejected)
    );
    assert!(run.failures()[0].take_panic_payload().is_none());
    assert!(fixture.is_empty());
}

#[cfg(feature = "async")]
#[test]
fn test_focused_async_copy_detects_unpublished_overwrite() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::CopyOverwriteKeepsTarget);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::CopyFallbackOverwriteRejected).await;
        assert!(!run.requirements_satisfied());
        assert_eq!(
            run.failures()[0].check(),
            Some(ContractCheckId::CopyFallbackOverwriteRejected)
        );
        assert!(run.failures()[0].take_panic_payload().is_none());
    });
}

/// Server-side copy supplies its own evidence and rejects unavailable
/// guarantees.
#[test]
fn test_focused_server_side_copy() {
    for fixture in [MemoryFixture::with_all_capabilities(), MemoryFixture::fallback_only()] {
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::CopyServerSide);
        assert_eq!(run.report().checks().len(), 1);
        run.assert_satisfied();
        assert!(fixture.is_empty());
    }
}

#[cfg(feature = "async")]
#[test]
fn test_focused_async_server_side_copy() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    run_controlled(async {
        for fixture in [AsyncMemoryFixture::new(), AsyncMemoryFixture::fallback_only()] {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::CopyServerSide).await;
            assert_eq!(run.report().checks().len(), 1);
            run.assert_satisfied();
        }
    });
}

/// A successful native copy does not establish the required server-side method.
#[test]
fn test_focused_server_side_detects_wrong_method() {
    let fixture = MemoryFixture::with_fault(self::common::MemoryFault::ServerSideCopyFallsBack);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::CopyServerSide);
    assert!(!run.requirements_satisfied());
    assert_eq!(run.failures()[0].check(), Some(ContractCheckId::CopyServerSide));
    assert!(run.failures()[0].take_panic_payload().is_none());
    assert!(fixture.is_empty());
}

#[cfg(feature = "async")]
#[test]
fn test_focused_async_server_side_detects_wrong_method() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::ServerSideCopyFallsBack);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::CopyServerSide).await;
        assert!(!run.requirements_satisfied());
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::CopyServerSide));
        assert!(run.failures()[0].take_panic_payload().is_none());
    });
}

/// An advertised server-side capability needs an independently prepared case.
#[test]
fn test_focused_server_side_unavailable_case() {
    let fixture = MemoryFixture::with_conditional_case_unavailable(crate::common::UnavailableScenario::Capability(
        qfs::metadata::FileSystemCapability::ServerSideCopy,
    ));
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::CopyServerSide);
    assert!(!run.requirements_satisfied());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        testkit::ContractCheckOutcome::Unverified { .. }
    ));
    assert!(run.failures().is_empty());
    assert!(fixture.is_empty());
}

#[cfg(feature = "async")]
#[test]
fn test_focused_async_server_side_unavailable_case() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::with_conditional_case_unavailable(
        crate::common::UnavailableScenario::Capability(qfs::metadata::FileSystemCapability::ServerSideCopy),
    );
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::CopyServerSide).await;
        assert!(!run.requirements_satisfied());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            testkit::ContractCheckOutcome::Unverified { .. }
        ));
        assert!(run.failures().is_empty());
    });
}

/// Each required file-copy guarantee runs independently on native and fallback
/// profiles.
#[test]
fn test_focused_strong_file_copy() {
    for id in [ContractCheckId::CopyAtomicFile, ContractCheckId::CopyDurableFile] {
        for fixture in [MemoryFixture::with_all_capabilities(), MemoryFixture::fallback_only()] {
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id);
            assert_eq!(run.report().checks().len(), 1);
            run.assert_satisfied();
            assert!(fixture.is_empty());
        }
    }
}

#[cfg(feature = "async")]
#[test]
fn test_focused_async_strong_file_copy() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    run_controlled(async {
        for id in [ContractCheckId::CopyAtomicFile, ContractCheckId::CopyDurableFile] {
            for fixture in [AsyncMemoryFixture::new(), AsyncMemoryFixture::fallback_only()] {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert_eq!(run.report().checks().len(), 1);
                run.assert_satisfied();
            }
        }
    });
}

/// Claimed atomicity and durability must match the actual required outcomes.
#[test]
fn test_focused_strong_file_copy_detects_false_guarantees() {
    for (id, fault) in [
        (
            ContractCheckId::CopyAtomicFile,
            self::common::MemoryFault::AtomicFileCopyNonAtomic,
        ),
        (
            ContractCheckId::CopyDurableFile,
            self::common::MemoryFault::DurableFileCopyNonDurable,
        ),
    ] {
        let fixture = MemoryFixture::with_fault(fault);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        assert!(!run.requirements_satisfied());
        assert_eq!(run.failures()[0].check(), Some(id));
        assert!(run.failures()[0].take_panic_payload().is_none());
        assert!(fixture.is_empty());
    }
}

#[cfg(feature = "async")]
#[test]
fn test_focused_async_strong_file_copy_detects_false_guarantees() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    run_controlled(async {
        for (id, fault) in [
            (
                ContractCheckId::CopyAtomicFile,
                AsyncMemoryFault::AtomicFileCopyNonAtomic,
            ),
            (
                ContractCheckId::CopyDurableFile,
                AsyncMemoryFault::DurableFileCopyNonDurable,
            ),
        ] {
            let fixture = AsyncMemoryFixture::with_fault(fault);
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(!run.requirements_satisfied());
            assert_eq!(run.failures()[0].check(), Some(id));
            assert!(run.failures()[0].take_panic_payload().is_none());
        }
    });
}

/// Missing preparation cannot satisfy an advertised file-copy guarantee.
#[test]
fn test_focused_strong_file_copy_unavailable_case() {
    use qubit_fs::metadata::FileSystemCapability;
    for (id, capability) in [
        (ContractCheckId::CopyAtomicFile, FileSystemCapability::AtomicFileCopy),
        (ContractCheckId::CopyDurableFile, FileSystemCapability::DurableFileCopy),
    ] {
        let fixture = MemoryFixture::with_conditional_case_unavailable(crate::common::UnavailableScenario::Capability(
            capability,
        ));
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        assert!(!run.requirements_satisfied());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            testkit::ContractCheckOutcome::Unverified { .. }
        ));
        assert!(run.failures().is_empty());
        assert!(fixture.is_empty());
    }
}

#[cfg(feature = "async")]
#[test]
fn test_focused_async_strong_file_copy_unavailable_case() {
    use qubit_fs::metadata::FileSystemCapability;
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    run_controlled(async {
        for (id, capability) in [
            (ContractCheckId::CopyAtomicFile, FileSystemCapability::AtomicFileCopy),
            (ContractCheckId::CopyDurableFile, FileSystemCapability::DurableFileCopy),
        ] {
            let fixture = AsyncMemoryFixture::with_conditional_case_unavailable(
                crate::common::UnavailableScenario::Capability(capability),
            );
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(!run.requirements_satisfied());
            assert!(matches!(
                run.report().checks()[0].outcome(),
                testkit::ContractCheckOutcome::Unverified { .. }
            ));
            assert!(run.failures().is_empty());
        }
    });
}

/// Required tree-copy guarantees must be independently executable.
#[test]
fn test_focused_strong_tree_copy() {
    for id in [ContractCheckId::CopyAtomicTree, ContractCheckId::CopyDurableTree] {
        for fixture in [MemoryFixture::with_all_capabilities(), MemoryFixture::fallback_only()] {
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id);
            assert_eq!(run.report().checks().len(), 1);
            run.assert_satisfied();
            assert!(fixture.is_empty());
        }
    }
}

#[cfg(feature = "async")]
#[test]
fn test_focused_async_strong_tree_copy() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    run_controlled(async {
        for id in [ContractCheckId::CopyAtomicTree, ContractCheckId::CopyDurableTree] {
            for fixture in [
                AsyncMemoryFixture::with_all_capabilities(),
                AsyncMemoryFixture::fallback_only(),
            ] {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert_eq!(run.report().checks().len(), 1);
                run.assert_satisfied();
            }
        }
    });
}

/// Tree publication, descendants and statistics are independently checked.
#[test]
fn test_focused_tree_copy_faults() {
    use self::common::MemoryFault;
    for (id, fault) in [
        (ContractCheckId::CopyAtomicTree, MemoryFault::AtomicTreeCopyNonAtomic),
        (ContractCheckId::CopyDurableTree, MemoryFault::DurableTreeCopyNonDurable),
        (ContractCheckId::CopyAtomicTree, MemoryFault::DirectoryCopyDropsChildren),
        (
            ContractCheckId::CopyDurableTree,
            MemoryFault::DirectoryCopyDropsChildren,
        ),
        (ContractCheckId::CopyAtomicTree, MemoryFault::TreeCopyWrongStats),
        (ContractCheckId::CopyDurableTree, MemoryFault::TreeCopyWrongStats),
    ] {
        let fixture = MemoryFixture::with_fault(fault);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        assert!(!run.requirements_satisfied(), "{id} / {fault:?}");
        assert_eq!(run.failures()[0].check(), Some(id));
        assert!(run.failures()[0].take_panic_payload().is_none());
    }
}

/// Tree preparation does not require the tested CreateDirectory operation.
#[test]
fn test_focused_tree_copy_without_directory_creation() {
    for id in [ContractCheckId::CopyAtomicTree, ContractCheckId::CopyDurableTree] {
        let fixture = MemoryFixture::tree_copy_without_directory_creation();
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        run.assert_satisfied();
        assert!(matches!(
            run.report().checks()[0].outcome(),
            testkit::ContractCheckOutcome::Passed
        ));
    }
}

/// Tree publication, descendants and statistics are independently checked.
#[cfg(feature = "async")]
#[test]
fn test_focused_async_tree_copy_faults() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    run_controlled(async {
        for (id, fault) in [
            (
                ContractCheckId::CopyAtomicTree,
                AsyncMemoryFault::AtomicTreeCopyNonAtomic,
            ),
            (
                ContractCheckId::CopyDurableTree,
                AsyncMemoryFault::DurableTreeCopyNonDurable,
            ),
            (
                ContractCheckId::CopyAtomicTree,
                AsyncMemoryFault::DirectoryCopyDropsChildren,
            ),
            (
                ContractCheckId::CopyDurableTree,
                AsyncMemoryFault::DirectoryCopyDropsChildren,
            ),
            (ContractCheckId::CopyAtomicTree, AsyncMemoryFault::TreeCopyWrongStats),
            (ContractCheckId::CopyDurableTree, AsyncMemoryFault::TreeCopyWrongStats),
        ] {
            let fixture = AsyncMemoryFixture::with_fault(fault);
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(!run.requirements_satisfied(), "{id} / {fault:?}");
            assert_eq!(run.failures()[0].check(), Some(id));
            assert!(run.failures()[0].take_panic_payload().is_none());
        }
    });
}

/// Tree preparation does not require the tested CreateDirectory operation.
#[cfg(feature = "async")]
#[test]
fn test_focused_async_tree_copy_without_directory_creation() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    run_controlled(async {
        for id in [ContractCheckId::CopyAtomicTree, ContractCheckId::CopyDurableTree] {
            let fixture = AsyncMemoryFixture::tree_copy_without_directory_creation();
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            run.assert_satisfied();
            assert!(matches!(
                run.report().checks()[0].outcome(),
                testkit::ContractCheckOutcome::Passed
            ));
        }
    });
}
