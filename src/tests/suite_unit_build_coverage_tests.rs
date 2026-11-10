// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use qubit_fs::copy::CopyMethod;
use qubit_fs::metadata::FileSystemCapability;

#[cfg(feature = "async")]
use super::super::AsyncFileSystemContractSuite;
#[cfg(feature = "async")]
use super::super::AsyncFileSystemFixture;
use super::super::ContractCheckId;
use super::super::FileSystemContractSuite;
use super::super::FileSystemFixture;
#[cfg(feature = "async")]
use super::common::AsyncMemoryFixture;
use super::common::MemoryFixture;
#[cfg(feature = "async")]
use super::common::async_memory_file_system::run_controlled;

#[test]
fn test_every_synchronous_check_executes_in_the_library_test_build() {
    let fixture = MemoryFixture::with_all_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.run_all().assert_satisfied();
    assert!(fixture.is_empty(), "the full synchronous suite must clean up");
}

#[test]
fn test_synchronous_suite_handles_default_unsupported_fixture_hooks() {
    let fixture = super::rustdoc_support::RustdocSyncFixture::new();
    assert!(matches!(
        fixture.seed_file("coverage-seed", b"payload"),
        Ok(super::super::FixtureSupport::Unsupported)
    ));
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_all();
    assert!(run.completed);
    assert!(run.failures().iter().all(|failure| failure.check().is_some()));
    assert!(
        run.report()
            .checks()
            .iter()
            .any(|check| matches!(check.outcome(), super::super::ContractCheckOutcome::Unverified { .. }))
    );
    assert!(run.cleanup().completed());
}

#[test]
fn test_synchronous_fixture_defaults_return_explicit_unsupported_evidence() {
    use super::super::CopyScenario;
    use super::super::DeleteScenario;
    use super::super::ReadScenario;
    use super::super::WriteScenario;

    let fixture = super::rustdoc_support::RustdocSyncFixture::new();
    let path = fixture.path("default-hook").expect("valid fixture path");
    assert!(!fixture.copy_fallback_only());
    assert_eq!(fixture.list_prefix(&path, "child").expect("prefix"), "child");
    assert!(matches!(
        fixture.snapshot_namespace_paths(),
        Ok(super::super::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.seed_file("file", b"bytes"),
        Ok(super::super::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.read_file(&path),
        Ok(super::super::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.exists_out_of_band(&path),
        Ok(super::super::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.write_file_out_of_band(&path, b"bytes"),
        Ok(super::super::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.resource_version(&path),
        Ok(super::super::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.stale_resource_version(&path),
        Ok(super::super::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.checksum_failure_case("file"),
        Ok(super::super::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.seed_empty_directory("directory"),
        Ok(super::super::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.seed_symlink("link"),
        Ok(super::super::FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.copy_fast_path_case(CopyMethod::ServerSide),
        Ok(super::super::FixtureSupport::Unsupported)
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
            Ok(super::super::FixturePreparation::Unavailable { .. })
        ));
    }
    assert!(matches!(
        fixture.prepare_delete(DeleteScenario::Basic, "file", b"bytes"),
        Ok(super::super::FixturePreparation::Unavailable { .. })
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
            Ok(super::super::FixturePreparation::Unavailable { .. })
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
            Ok(super::super::FixturePreparation::Unavailable { .. })
        ));
    }
    for scenario in [WriteScenario::Create, WriteScenario::Abort, WriteScenario::Durable] {
        assert!(matches!(
            fixture.prepare_write(scenario, "file", b"bytes"),
            Ok(super::super::FixturePreparation::Ready(_))
        ));
    }
}

#[test]
fn test_synchronous_advertised_capability_requires_seed_support() {
    let fixture = MemoryFixture::with_unavailable_seed_calls(None, Some(1));
    let mut suite = FileSystemContractSuite::new(&fixture);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = suite.required_seed("required-seed", b"payload", "coverage probe");
    }));
    assert!(result.is_err());
}

#[test]
fn test_synchronous_publication_and_metadata_errors_keep_the_selected_check() {
    for (id, fault, message) in [
        (
            ContractCheckId::AppendBasic,
            super::common::MemoryFault::WriteFails,
            "append publication failed",
        ),
        (
            ContractCheckId::WriteBasic,
            super::common::MemoryFault::WriteFails,
            "write/basic: write failed",
        ),
        (
            ContractCheckId::WriteReplace,
            super::common::MemoryFault::WriteFails,
            "replacement publication failed",
        ),
        (
            ContractCheckId::WriteIfAbsent,
            super::common::MemoryFault::WriteFails,
            "conditional write first publication failed",
        ),
        (
            ContractCheckId::WriteIfMatch,
            super::common::MemoryFault::WriteFails,
            "conditional write first publication failed",
        ),
        (
            ContractCheckId::DirectoryCreate,
            super::common::MemoryFault::CreateDirectoryFails,
            "directory creation failed",
        ),
        (
            ContractCheckId::CopyBasic,
            super::common::MemoryFault::CopyFails,
            "copy execution failed",
        ),
        (
            ContractCheckId::CopyFallbackOverwriteRejected,
            super::common::MemoryFault::CopyOverwriteFails,
            "copy conflict execution failed",
        ),
        (
            ContractCheckId::ReadRange,
            super::common::MemoryFault::ReadFailsOnEmptyRange,
            "empty range observation failed",
        ),
        (
            ContractCheckId::RepresentationEmpty,
            super::common::MemoryFault::CleanupStatError,
            "representation metadata observation failed",
        ),
        (
            ContractCheckId::StatFileKind,
            super::common::MemoryFault::CleanupStatError,
            "seeded file stat failed",
        ),
    ] {
        let fixture = MemoryFixture::with_fault_and_all_capabilities(fault);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);

        assert_eq!(
            run.failures().first().and_then(|failure| failure.check()),
            Some(id),
            "{id}: {:?}; report: {:?}",
            run.failures(),
            run.report().checks()
        );
        assert!(
            run.failures()
                .iter()
                .any(|failure| failure.to_string().contains(message)),
            "{id}: expected failure containing {message:?}, got {:?}",
            run.failures()
        );
    }
}

#[test]
fn test_synchronous_public_suite_helpers_seed_and_track_resources() {
    let fixture = MemoryFixture::with_all_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);

    let path = suite.path("support-path");
    assert!(path.as_str().ends_with("support-path"));
    assert!(matches!(
        suite.seed("support-seed", b"seeded"),
        super::super::FixtureSupport::Supported(_)
    ));
    let required = suite.required_seed("required-support-seed", b"required", "coverage probe");
    assert!(required.as_str().ends_with("required-support-seed"));

    suite.finish();
    assert!(
        fixture.is_empty(),
        "public suite helpers must register seeded paths for cleanup"
    );
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
    let fixture = MemoryFixture::with_conditional_case_unavailable(super::common::UnavailableScenario::Capability(
        FileSystemCapability::ServerSideCopy,
    ));
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::CopyServerSide);
    assert!(run.completed);
    assert!(run.failures().is_empty());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        super::super::ContractCheckOutcome::Unverified { .. }
    ));
    assert!(run.cleanup().completed());
}

#[test]
fn test_synchronous_server_side_copy_rejects_an_unadvertised_capability() {
    let fixture = MemoryFixture::without_optional_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::CopyServerSide);
    assert!(run.completed);
    assert!(run.failures().is_empty());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        super::super::ContractCheckOutcome::RejectedAsExpected
    ));
}

#[test]
fn test_synchronous_temp_directory_paths_retain_creation_and_cleanup_failures() {
    for (fault, id) in [
        (
            super::common::MemoryFault::TempCreationFails,
            ContractCheckId::TempDirectory,
        ),
        (
            super::common::MemoryFault::TempCleanupFailsOnce,
            ContractCheckId::TempDirectory,
        ),
        (
            super::common::MemoryFault::TempKeepFails,
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
fn test_synchronous_repeated_temp_lifecycle_retains_each_interleaved_cleanup_failure() {
    for call in [1, 2, 3, 4, 5, 6, 7, 8] {
        let fixture = MemoryFixture::with_temp_cleanup_error_call(call);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempRepeatedLifecycle);
        assert!(
            run.failures()
                .iter()
                .any(|failure| failure.check() == Some(ContractCheckId::TempRepeatedLifecycle)),
            "cleanup call {call} must be attributed to the repeated lifecycle"
        );
        assert!(
            fixture.is_empty(),
            "cleanup call {call} must be retried during teardown"
        );
    }
}

#[test]
fn test_synchronous_temp_directory_preserves_fixture_observation_errors() {
    let hooks = [
        super::common::shared_model::FixtureHook::WriteFile,
        super::common::shared_model::FixtureHook::ReadFile,
    ];
    for id in [ContractCheckId::TempDirectory, ContractCheckId::TempRepeatedLifecycle] {
        let baseline = MemoryFixture::with_all_capabilities();
        let mut suite = FileSystemContractSuite::new(&baseline);
        let _ = suite.run_check(id);
        for hook in hooks {
            for call in 1..=baseline.fixture_hook_call_count(hook) {
                let fixture = MemoryFixture::with_fixture_hook_error(hook, call);
                let mut suite = FileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id);
                assert!(!run.failures().is_empty(), "{id}, {hook:?} call {call} must fail");
                assert!(run.failures().iter().all(|failure| failure.check() == Some(id)));
            }
        }
        for call in 1..=baseline.exists_call_count() {
            let fixture = MemoryFixture::with_exists_error_call(call);
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id);
            assert!(
                !run.failures().is_empty(),
                "{id} existence observation {call} must fail"
            );
            assert!(run.failures().iter().all(|failure| failure.check() == Some(id)));
        }
    }
}

#[test]
fn test_synchronous_temp_directory_marks_unsupported_content_evidence_unverified() {
    for fault in [
        super::common::MemoryFault::TempWriteHookUnsupported,
        super::common::MemoryFault::TempReadHookUnsupported,
    ] {
        let fixture = MemoryFixture::with_fault(fault);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempDirectory);
        assert!(run.completed, "{fault:?} check must complete");
        assert!(
            run.failures().is_empty(),
            "{fault:?} is unavailable evidence, not a provider failure"
        );
        assert!(matches!(
            run.report().checks()[0].outcome(),
            super::super::ContractCheckOutcome::Unverified { .. }
        ));
        assert!(run.cleanup().completed(), "{fault:?} cleanup must be reported");
    }
}

#[test]
fn test_synchronous_suite_preserves_indexed_fixture_hook_failures() {
    let hooks = [
        super::common::shared_model::FixtureHook::Snapshot,
        super::common::shared_model::FixtureHook::ReadFile,
        super::common::shared_model::FixtureHook::WriteFile,
        super::common::shared_model::FixtureHook::ResourceVersion,
        super::common::shared_model::FixtureHook::StaleResourceVersion,
        super::common::shared_model::FixtureHook::ChecksumFailureCase,
        super::common::shared_model::FixtureHook::SeedSymlink,
        super::common::shared_model::FixtureHook::CopyFastPathCase,
    ];
    for id in ContractCheckId::ALL
        .iter()
        .copied()
        .filter(|id| id.supports_synchronous())
    {
        let baseline = MemoryFixture::with_all_capabilities();
        let mut suite = FileSystemContractSuite::new(&baseline);
        let _ = suite.run_check(id);
        for hook in hooks {
            for call in 1..=baseline.fixture_hook_call_count(hook) {
                let fixture = MemoryFixture::with_fixture_hook_error(hook, call);
                let mut suite = FileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id);
                assert!(
                    run.report().checks().iter().any(|check| check.id() == id),
                    "{id}, {hook:?} call {call} must retain its check"
                );
                assert!(
                    run.failures()
                        .iter()
                        .all(|failure| failure.check().is_none_or(|check| check == id))
                );
            }
        }
    }
}

#[test]
fn test_synchronous_checks_retain_every_indexed_seed_failure() {
    for id in ContractCheckId::ALL
        .iter()
        .copied()
        .filter(|id| id.supports_synchronous())
    {
        let baseline = MemoryFixture::with_all_capabilities();
        let mut suite = FileSystemContractSuite::new(&baseline);
        let run = suite.run_check(id);
        assert!(run.requirements_satisfied(), "baseline {id}: {:?}", run.failures());
        let file_calls = baseline.seed_file_call_count();
        let directory_calls = baseline.seed_directory_call_count();

        for (file_call, directory_call) in (1..=file_calls)
            .map(|call| (Some(call), None))
            .chain((1..=directory_calls).map(|call| (None, Some(call))))
        {
            let fixture = MemoryFixture::with_seed_error_calls(file_call, directory_call);
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id);
            assert!(
                run.failures().iter().any(|failure| failure.check() == Some(id)),
                "{id}, file seed {file_call:?}, directory seed {directory_call:?}"
            );
            assert!(fixture.is_empty(), "{id} seed failure leaked resources");
        }
    }
}

#[test]
fn test_synchronous_checks_record_every_indexed_unavailable_seed() {
    for id in ContractCheckId::ALL
        .iter()
        .copied()
        .filter(|id| id.supports_synchronous())
    {
        let baseline = MemoryFixture::with_all_capabilities();
        let mut suite = FileSystemContractSuite::new(&baseline);
        let _ = suite.run_check(id);
        let file_calls = baseline.seed_file_call_count();
        let directory_calls = baseline.seed_directory_call_count();

        for (directory_call, file_call) in (1..=file_calls)
            .map(|call| (None, Some(call)))
            .chain((1..=directory_calls).map(|call| (Some(call), None)))
        {
            let fixture = MemoryFixture::with_unavailable_seed_calls(directory_call, file_call);
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id);
            assert!(
                run.completed,
                "{id}, file seed {file_call:?}, directory seed {directory_call:?}"
            );
            assert!(
                run.failures()
                    .iter()
                    .all(|failure| failure.check().is_none_or(|check| check == id)),
                "{id}, file seed {file_call:?}, directory seed {directory_call:?}"
            );
        }
    }
}

#[test]
fn test_synchronous_checks_retain_every_indexed_existence_failure() {
    for id in ContractCheckId::ALL
        .iter()
        .copied()
        .filter(|id| id.supports_synchronous())
    {
        let baseline = MemoryFixture::with_all_capabilities();
        let mut suite = FileSystemContractSuite::new(&baseline);
        let _ = suite.run_check(id);
        for call in 1..=baseline.exists_call_count() {
            let fixture = MemoryFixture::with_exists_error_call(call);
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id);
            assert!(
                run.failures()
                    .iter()
                    .all(|failure| failure.check().is_none_or(|check| check == id)),
                "{id}, existence observation {call}"
            );
        }
    }
}

#[test]
fn test_synchronous_checks_record_indexed_unsupported_fixture_evidence() {
    let hooks = [
        super::common::shared_model::FixtureHook::Snapshot,
        super::common::shared_model::FixtureHook::ReadFile,
        super::common::shared_model::FixtureHook::WriteFile,
        super::common::shared_model::FixtureHook::Exists,
        super::common::shared_model::FixtureHook::ResourceVersion,
        super::common::shared_model::FixtureHook::StaleResourceVersion,
        super::common::shared_model::FixtureHook::ChecksumFailureCase,
        super::common::shared_model::FixtureHook::SeedSymlink,
        super::common::shared_model::FixtureHook::CopyFastPathCase,
    ];
    for id in ContractCheckId::ALL
        .iter()
        .copied()
        .filter(|id| id.supports_synchronous())
    {
        let baseline = MemoryFixture::with_all_capabilities();
        let mut suite = FileSystemContractSuite::new(&baseline);
        let _ = suite.run_check(id);
        for hook in hooks {
            for call in 1..=baseline.fixture_hook_call_count(hook) {
                let fixture = MemoryFixture::with_unsupported_fixture_hook(hook, call);
                let mut suite = FileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id);
                assert!(run.completed, "{id}, {hook:?} call {call} must complete");
                assert!(
                    run.failures()
                        .iter()
                        .all(|failure| failure.check().is_none_or(|check| check == id)),
                    "{id}, {hook:?} call {call} must retain attribution"
                );
            }
        }
    }
}

#[test]
fn test_synchronous_temporary_lifecycle_records_indexed_unsupported_evidence() {
    let hooks = [
        super::common::shared_model::FixtureHook::ReadFile,
        super::common::shared_model::FixtureHook::WriteFile,
        super::common::shared_model::FixtureHook::Exists,
    ];
    for id in [
        ContractCheckId::TempDirectory,
        ContractCheckId::TempAtomic,
        ContractCheckId::TempRepeatedLifecycle,
    ] {
        let baseline = MemoryFixture::with_all_capabilities();
        let mut suite = FileSystemContractSuite::new(&baseline);
        let _ = suite.run_check(id);
        for hook in hooks {
            for call in 1..=baseline.fixture_hook_call_count(hook) {
                let fixture = MemoryFixture::with_unsupported_fixture_hook(hook, call);
                let mut suite = FileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id);
                assert!(run.completed, "{id}, {hook:?} call {call} must complete");
                assert!(
                    run.failures()
                        .iter()
                        .all(|failure| failure.check().is_none_or(|check| check == id))
                );
                assert_eq!(run.report().checks().len(), 1);
            }
        }
    }
}

#[test]
fn test_synchronous_temporary_file_records_indexed_unsupported_evidence() {
    let hooks = [
        super::common::shared_model::FixtureHook::ReadFile,
        super::common::shared_model::FixtureHook::WriteFile,
        super::common::shared_model::FixtureHook::Exists,
    ];
    for id in [
        ContractCheckId::TempFile,
        ContractCheckId::TempAtomic,
        ContractCheckId::TempRepeatedLifecycle,
    ] {
        let baseline = MemoryFixture::with_all_capabilities();
        let mut suite = FileSystemContractSuite::new(&baseline);
        let _ = suite.run_check(id);
        for hook in hooks {
            for call in 1..=baseline.fixture_hook_call_count(hook) {
                let fixture = MemoryFixture::with_unsupported_fixture_hook(hook, call);
                let mut suite = FileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id);
                assert!(run.completed, "{id}, {hook:?} call {call} must complete");
                assert_eq!(run.report().checks().len(), 1);
                assert!(
                    run.failures()
                        .iter()
                        .all(|failure| failure.check().is_none_or(|check| check == id))
                );
            }
        }
    }
}

#[test]
fn test_synchronous_read_range_checks_cover_one_and_two_byte_limits() {
    for maximum in [1, 2] {
        let limits = qubit_fs::metadata::FileSystemLimits::unknown()
            .with_max_read_range_bytes(qubit_fs::metadata::FileSystemLimit::Maximum(maximum));
        let fixture = MemoryFixture::with_limits(limits);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::ReadRange);
        assert!(
            run.requirements_satisfied(),
            "maximum {maximum}, ReadRange: {:?}",
            run.failures()
        );
        assert_eq!(
            fixture
                .read_requests()
                .into_iter()
                .filter(|request| *request == (Some(5), Some(maximum)))
                .count(),
            3,
            "range window must be requested for base, accepted-budget and rejected-budget reads"
        );
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::ReadRangeLimit);
        assert!(
            run.requirements_satisfied(),
            "maximum {maximum}, ReadRangeLimit: {:?}",
            run.failures()
        );
        assert!(fixture.is_empty(), "maximum {maximum} read checks must clean up");
    }
}

#[test]
fn test_synchronous_bounded_write_limit_observes_rejected_target() {
    let limits = qubit_fs::metadata::FileSystemLimits::unknown()
        .with_max_write_bytes(qubit_fs::metadata::FileSystemLimit::Maximum(4));
    let fixture = MemoryFixture::with_fault_and_limits(super::common::MemoryFault::None, limits);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteLimit);
    assert!(run.requirements_satisfied(), "{:?}", run.failures());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        super::super::ContractCheckOutcome::Passed
    ));
}

#[test]
fn test_synchronous_bounded_write_limit_does_not_pass_without_baseline() {
    let limits = qubit_fs::metadata::FileSystemLimits::unknown()
        .with_max_write_bytes(qubit_fs::metadata::FileSystemLimit::Maximum(4));
    let fixture = MemoryFixture::with_unsupported_fixture_hook_and_limits(
        super::common::shared_model::FixtureHook::Exists,
        1,
        limits,
    );
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteLimit);
    assert!(matches!(
        run.report().checks()[0].outcome(),
        super::super::ContractCheckOutcome::Unverified { .. }
    ));
}

#[test]
fn test_synchronous_read_range_attributes_budget_window_reader_error() {
    let limits = qubit_fs::metadata::FileSystemLimits::unknown()
        .with_max_read_range_bytes(qubit_fs::metadata::FileSystemLimit::Maximum(4));
    let fixture = MemoryFixture::with_reader_error_call_and_limits(2, limits);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::ReadRange);
    assert!(!run.requirements_satisfied());
    assert_eq!(run.failures()[0].check(), Some(ContractCheckId::ReadRange));
}

#[test]
fn test_synchronous_bounded_write_limit_attributes_setup_error() {
    let limits = qubit_fs::metadata::FileSystemLimits::unknown()
        .with_max_write_bytes(qubit_fs::metadata::FileSystemLimit::Maximum(4));
    let fixture = MemoryFixture::with_path_error_call_and_limits(1, limits);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteLimit);
    assert!(!run.requirements_satisfied());
    assert_eq!(run.failures()[0].check(), Some(ContractCheckId::WriteLimit));
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

#[test]
fn test_synchronous_capable_profile_preserves_every_path_failure() {
    for id in ContractCheckId::ALL
        .iter()
        .copied()
        .filter(|id| id.supports_synchronous())
    {
        let baseline = MemoryFixture::with_all_capabilities();
        let mut suite = FileSystemContractSuite::new(&baseline);
        let _ = suite.run_check(id);
        for call in 1..=baseline.path_call_count() {
            let fixture = MemoryFixture::with_path_error_call(call);
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id);
            assert!(!run.failures().is_empty(), "{id}, path {call}");
            assert!(run.failures().iter().all(|failure| failure.check() == Some(id)));
            assert!(fixture.is_empty(), "{id}, path {call} leaked resources");
        }
    }
}

#[cfg(feature = "async")]
#[test]
fn test_every_asynchronous_check_executes_in_the_library_test_build() {
    let fixture = AsyncMemoryFixture::with_all_capabilities();
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        suite.run_all().await.assert_satisfied();
    });
    assert!(fixture.is_empty(), "the full asynchronous suite must clean up");
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_suite_handles_default_unsupported_fixture_hooks() {
    let fixture = super::rustdoc_support::RustdocAsyncFixture::new();
    assert!(matches!(
        run_controlled(fixture.seed_file("coverage-seed", b"payload")),
        Ok(super::super::FixtureSupport::Unsupported)
    ));
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_all().await;
        assert!(run.completed);
        assert!(run.failures().iter().all(|failure| failure.check().is_some()));
        assert!(run.cleanup().completed());
    });
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_fixture_defaults_return_explicit_unsupported_evidence() {
    use super::super::CopyScenario;
    use super::super::DeleteScenario;
    use super::super::ReadScenario;
    use super::super::WriteScenario;

    let fixture = super::rustdoc_support::RustdocAsyncFixture::new();
    let path = fixture.path("default-hook").expect("valid fixture path");
    run_controlled(async {
        assert!(!fixture.copy_fallback_only());
        assert_eq!(fixture.list_prefix(&path, "child").expect("prefix"), "child");
        assert!(matches!(
            fixture.snapshot_namespace_paths().await,
            Ok(super::super::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.seed_file("file", b"bytes").await,
            Ok(super::super::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.read_file(&path).await,
            Ok(super::super::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.exists_out_of_band(&path).await,
            Ok(super::super::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.write_file_out_of_band(&path, b"bytes").await,
            Ok(super::super::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.resource_version(&path).await,
            Ok(super::super::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.stale_resource_version(&path).await,
            Ok(super::super::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.checksum_failure_case("file").await,
            Ok(super::super::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.seed_empty_directory("directory").await,
            Ok(super::super::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.seed_symlink("link").await,
            Ok(super::super::FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            fixture.copy_fast_path_case(CopyMethod::ServerSide).await,
            Ok(super::super::FixtureSupport::Unsupported)
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
                Ok(super::super::FixturePreparation::Unavailable { .. })
            ));
        }
        assert!(matches!(
            fixture.prepare_delete(DeleteScenario::Basic, "file", b"bytes").await,
            Ok(super::super::FixturePreparation::Unavailable { .. })
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
                Ok(super::super::FixturePreparation::Unavailable { .. })
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
                Ok(super::super::FixturePreparation::Unavailable { .. })
            ));
        }
        for scenario in [WriteScenario::Create, WriteScenario::Abort, WriteScenario::Durable] {
            assert!(matches!(
                fixture.prepare_write(scenario, "file", b"bytes").await,
                Ok(super::super::FixturePreparation::Ready(_))
            ));
        }
    });
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_advertised_capability_requires_seed_support() {
    let fixture = AsyncMemoryFixture::with_unavailable_seed_calls(None, Some(1));
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let result = super::super::internal::catch_unwind_future(suite.required_seed(
            "required-seed",
            b"payload",
            "coverage probe",
        ))
        .await;
        assert!(result.is_err());
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
        let fixture = AsyncMemoryFixture::with_cancellation_limits(qubit_fs::metadata::FileSystemLimits::unknown());
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(run.requirements_satisfied(), "{id}: {:?}", run.failures());
            assert!(
                matches!(
                    run.report()
                        .checks()
                        .iter()
                        .find(|check| check.id() == id)
                        .map(|check| check.outcome()),
                    Some(super::super::ContractCheckOutcome::Passed)
                ),
                "{id} must execute its stage probe"
            );
        });
        assert!(fixture.is_empty(), "{id}: fixture leaked resources");
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_server_side_copy_reports_unavailable_fixture_evidence() {
    let fixture = AsyncMemoryFixture::with_conditional_case_unavailable(
        super::common::UnavailableScenario::Capability(FileSystemCapability::ServerSideCopy),
    );
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::CopyServerSide).await;
        assert!(run.completed);
        assert!(run.failures().is_empty());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            super::super::ContractCheckOutcome::Unverified { .. }
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
        assert!(run.completed);
        assert!(run.failures().is_empty());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            super::super::ContractCheckOutcome::RejectedAsExpected
        ));
    });
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_temp_directory_paths_retain_creation_and_cleanup_failures() {
    for (fault, id) in [
        (
            super::common::AsyncMemoryFault::TempCreationFails,
            ContractCheckId::TempDirectory,
        ),
        (
            super::common::AsyncMemoryFault::TempCleanupFailsOnce,
            ContractCheckId::TempDirectory,
        ),
        (
            super::common::AsyncMemoryFault::TempKeepFails,
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
fn test_asynchronous_repeated_temp_lifecycle_retains_each_interleaved_cleanup_failure() {
    for call in [1, 2, 3, 4, 5, 6, 7, 8] {
        let fixture = AsyncMemoryFixture::with_temp_cleanup_error_call(call);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::TempRepeatedLifecycle).await;
            assert!(
                run.failures()
                    .iter()
                    .any(|failure| failure.check() == Some(ContractCheckId::TempRepeatedLifecycle)),
                "cleanup call {call} must be attributed to the repeated lifecycle"
            );
        });
        assert!(
            fixture.is_empty(),
            "cleanup call {call} must be retried during teardown"
        );
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_temp_directory_preserves_fixture_observation_errors() {
    let hooks = [
        super::common::shared_model::FixtureHook::WriteFile,
        super::common::shared_model::FixtureHook::ReadFile,
    ];
    for id in [ContractCheckId::TempDirectory, ContractCheckId::TempRepeatedLifecycle] {
        let baseline = AsyncMemoryFixture::with_all_capabilities();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&baseline);
            let _ = suite.run_check(id).await;
        });
        for hook in hooks {
            for call in 1..=baseline.fixture_hook_call_count(hook) {
                let fixture = AsyncMemoryFixture::with_fixture_hook_error(hook, call);
                run_controlled(async {
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert!(!run.failures().is_empty(), "{id}, {hook:?} call {call} must fail");
                    assert!(run.failures().iter().all(|failure| failure.check() == Some(id)));
                });
            }
        }
        for call in 1..=baseline.exists_call_count() {
            let fixture = AsyncMemoryFixture::with_exists_error_call(call);
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert!(
                    !run.failures().is_empty(),
                    "{id} existence observation {call} must fail"
                );
                assert!(run.failures().iter().all(|failure| failure.check() == Some(id)));
            });
        }
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_temp_directory_marks_unsupported_content_evidence_unverified() {
    for fault in [
        super::common::async_memory_file_system::AsyncMemoryFault::TempWriteHookUnsupported,
        super::common::async_memory_file_system::AsyncMemoryFault::TempReadHookUnsupported,
    ] {
        let fixture = AsyncMemoryFixture::with_fault(fault);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::TempDirectory).await;
            assert!(run.completed, "{fault:?} check must complete");
            assert!(
                run.failures().is_empty(),
                "{fault:?} is unavailable evidence, not a provider failure"
            );
            assert!(matches!(
                run.report().checks()[0].outcome(),
                super::super::ContractCheckOutcome::Unverified { .. }
            ));
            assert!(run.cleanup().completed(), "{fault:?} cleanup must be reported");
        });
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_suite_preserves_indexed_fixture_hook_failures() {
    let hooks = [
        super::common::shared_model::FixtureHook::Snapshot,
        super::common::shared_model::FixtureHook::ReadFile,
        super::common::shared_model::FixtureHook::WriteFile,
        super::common::shared_model::FixtureHook::ResourceVersion,
        super::common::shared_model::FixtureHook::StaleResourceVersion,
        super::common::shared_model::FixtureHook::ChecksumFailureCase,
        super::common::shared_model::FixtureHook::SeedSymlink,
        super::common::shared_model::FixtureHook::CopyFastPathCase,
    ];
    for id in ContractCheckId::ALL
        .iter()
        .copied()
        .filter(|id| id.supports_synchronous())
    {
        let baseline = AsyncMemoryFixture::with_all_capabilities();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&baseline);
            let _ = suite.run_check(id).await;
        });
        for hook in hooks {
            for call in 1..=baseline.fixture_hook_call_count(hook) {
                let fixture = AsyncMemoryFixture::with_fixture_hook_error(hook, call);
                run_controlled(async {
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert!(
                        run.report().checks().iter().any(|check| check.id() == id),
                        "{id}, {hook:?} call {call} must retain its check"
                    );
                    assert!(
                        run.failures()
                            .iter()
                            .all(|failure| failure.check().is_none_or(|check| check == id))
                    );
                });
            }
        }
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_checks_retain_every_indexed_seed_failure() {
    for id in ContractCheckId::ALL.iter().copied() {
        let baseline = AsyncMemoryFixture::with_all_capabilities();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&baseline);
            let run = suite.run_check(id).await;
            assert!(run.requirements_satisfied(), "baseline {id}: {:?}", run.failures());
        });
        let file_calls = baseline.seed_file_call_count();
        let directory_calls = baseline.seed_directory_call_count();

        for (file_call, directory_call) in (1..=file_calls)
            .map(|call| (Some(call), None))
            .chain((1..=directory_calls).map(|call| (None, Some(call))))
        {
            let fixture = AsyncMemoryFixture::with_seed_error_calls(file_call, directory_call);
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert!(
                    run.failures().iter().any(|failure| failure.check() == Some(id)),
                    "{id}, file seed {file_call:?}, directory seed {directory_call:?}"
                );
            });
            assert!(fixture.is_empty(), "{id} seed failure leaked resources");
        }
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_checks_record_every_indexed_unavailable_seed() {
    for id in ContractCheckId::ALL.iter().copied() {
        let baseline = AsyncMemoryFixture::with_all_capabilities();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&baseline);
            let _ = suite.run_check(id).await;
        });
        let file_calls = baseline.seed_file_call_count();
        let directory_calls = baseline.seed_directory_call_count();

        for (directory_call, file_call) in (1..=file_calls)
            .map(|call| (None, Some(call)))
            .chain((1..=directory_calls).map(|call| (Some(call), None)))
        {
            let fixture = AsyncMemoryFixture::with_unavailable_seed_calls(directory_call, file_call);
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert!(
                    run.completed,
                    "{id}, file seed {file_call:?}, directory seed {directory_call:?}"
                );
                assert!(
                    run.failures()
                        .iter()
                        .all(|failure| failure.check().is_none_or(|check| check == id)),
                    "{id}, file seed {file_call:?}, directory seed {directory_call:?}"
                );
            });
        }
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_checks_retain_every_indexed_existence_failure() {
    for id in ContractCheckId::ALL.iter().copied() {
        let baseline = AsyncMemoryFixture::with_all_capabilities();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&baseline);
            let _ = suite.run_check(id).await;
        });
        for call in 1..=baseline.exists_call_count() {
            let fixture = AsyncMemoryFixture::with_exists_error_call(call);
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert!(
                    run.failures()
                        .iter()
                        .all(|failure| failure.check().is_none_or(|check| check == id)),
                    "{id}, existence observation {call}"
                );
            });
        }
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_checks_record_indexed_unsupported_fixture_evidence() {
    let hooks = [
        super::common::shared_model::FixtureHook::Snapshot,
        super::common::shared_model::FixtureHook::ReadFile,
        super::common::shared_model::FixtureHook::WriteFile,
        super::common::shared_model::FixtureHook::Exists,
        super::common::shared_model::FixtureHook::ResourceVersion,
        super::common::shared_model::FixtureHook::StaleResourceVersion,
        super::common::shared_model::FixtureHook::ChecksumFailureCase,
        super::common::shared_model::FixtureHook::SeedSymlink,
        super::common::shared_model::FixtureHook::CopyFastPathCase,
    ];
    for id in ContractCheckId::ALL.iter().copied() {
        let baseline = AsyncMemoryFixture::with_all_capabilities();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&baseline);
            let _ = suite.run_check(id).await;
        });
        for hook in hooks {
            for call in 1..=baseline.fixture_hook_call_count(hook) {
                let fixture = AsyncMemoryFixture::with_unsupported_fixture_hook(hook, call);
                run_controlled(async {
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert!(run.completed, "{id}, {hook:?} call {call} must complete");
                    assert!(
                        run.failures()
                            .iter()
                            .all(|failure| failure.check().is_none_or(|check| check == id)),
                        "{id}, {hook:?} call {call} must retain attribution"
                    );
                });
            }
        }
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_temporary_lifecycle_records_indexed_unsupported_evidence() {
    let hooks = [
        super::common::shared_model::FixtureHook::ReadFile,
        super::common::shared_model::FixtureHook::WriteFile,
        super::common::shared_model::FixtureHook::Exists,
    ];
    for id in [
        ContractCheckId::TempDirectory,
        ContractCheckId::TempAtomic,
        ContractCheckId::TempRepeatedLifecycle,
    ] {
        let baseline = AsyncMemoryFixture::with_all_capabilities();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&baseline);
            let _ = suite.run_check(id).await;
        });
        for hook in hooks {
            for call in 1..=baseline.fixture_hook_call_count(hook) {
                let fixture = AsyncMemoryFixture::with_unsupported_fixture_hook(hook, call);
                run_controlled(async {
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert!(run.completed, "{id}, {hook:?} call {call} must complete");
                    assert!(
                        run.failures()
                            .iter()
                            .all(|failure| failure.check().is_none_or(|check| check == id))
                    );
                    assert_eq!(run.report().checks().len(), 1);
                });
            }
        }
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_temporary_file_records_indexed_unsupported_evidence() {
    let hooks = [
        super::common::shared_model::FixtureHook::ReadFile,
        super::common::shared_model::FixtureHook::WriteFile,
        super::common::shared_model::FixtureHook::Exists,
    ];
    for id in [
        ContractCheckId::TempFile,
        ContractCheckId::TempAtomic,
        ContractCheckId::TempRepeatedLifecycle,
    ] {
        let baseline = AsyncMemoryFixture::with_all_capabilities();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&baseline);
            let _ = suite.run_check(id).await;
        });
        for hook in hooks {
            for call in 1..=baseline.fixture_hook_call_count(hook) {
                let fixture = AsyncMemoryFixture::with_unsupported_fixture_hook(hook, call);
                run_controlled(async {
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert!(run.completed, "{id}, {hook:?} call {call} must complete");
                    assert_eq!(run.report().checks().len(), 1);
                    assert!(
                        run.failures()
                            .iter()
                            .all(|failure| failure.check().is_none_or(|check| check == id))
                    );
                });
            }
        }
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_read_range_checks_cover_one_and_two_byte_limits() {
    for maximum in [1, 2] {
        let limits = qubit_fs::metadata::FileSystemLimits::unknown()
            .with_max_read_range_bytes(qubit_fs::metadata::FileSystemLimit::Maximum(maximum));
        let fixture = AsyncMemoryFixture::with_fault_and_limits(super::common::AsyncMemoryFault::None, limits);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::ReadRange).await;
            assert!(
                run.requirements_satisfied(),
                "maximum {maximum}, ReadRange: {:?}",
                run.failures()
            );
            assert_eq!(
                fixture
                    .read_requests()
                    .into_iter()
                    .filter(|request| *request == (Some(5), Some(maximum)))
                    .count(),
                3,
                "range window must be requested for base, accepted-budget and rejected-budget reads"
            );
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::ReadRangeLimit).await;
            assert!(
                run.requirements_satisfied(),
                "maximum {maximum}, ReadRangeLimit: {:?}",
                run.failures()
            );
        });
        assert!(fixture.is_empty(), "maximum {maximum} read checks must clean up");
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_bounded_write_limit_observes_rejected_target() {
    let limits = qubit_fs::metadata::FileSystemLimits::unknown()
        .with_max_write_bytes(qubit_fs::metadata::FileSystemLimit::Maximum(4));
    let fixture = AsyncMemoryFixture::with_fault_and_limits(super::common::AsyncMemoryFault::None, limits);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::WriteLimit).await;
        assert!(run.requirements_satisfied(), "{:?}", run.failures());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            super::super::ContractCheckOutcome::Passed
        ));
    });
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_bounded_write_limit_does_not_pass_without_baseline() {
    let limits = qubit_fs::metadata::FileSystemLimits::unknown()
        .with_max_write_bytes(qubit_fs::metadata::FileSystemLimit::Maximum(4));
    let fixture = AsyncMemoryFixture::with_unsupported_fixture_hook_and_limits(
        super::common::shared_model::FixtureHook::Exists,
        1,
        limits,
    );
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::WriteLimit).await;
        assert!(matches!(
            run.report().checks()[0].outcome(),
            super::super::ContractCheckOutcome::Unverified { .. }
        ));
    });
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_read_range_attributes_budget_window_reader_error() {
    let limits = qubit_fs::metadata::FileSystemLimits::unknown()
        .with_max_read_range_bytes(qubit_fs::metadata::FileSystemLimit::Maximum(4));
    let fixture = AsyncMemoryFixture::with_reader_error_call_and_limits(2, limits);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::ReadRange).await;
        assert!(!run.requirements_satisfied());
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::ReadRange));
    });
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_bounded_write_limit_attributes_setup_error() {
    let limits = qubit_fs::metadata::FileSystemLimits::unknown()
        .with_max_write_bytes(qubit_fs::metadata::FileSystemLimit::Maximum(4));
    let fixture = AsyncMemoryFixture::with_path_error_call_and_limits(1, limits);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::WriteLimit).await;
        assert!(!run.requirements_satisfied());
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::WriteLimit));
    });
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_copy_cancellation_covers_every_supported_stage() {
    for id in [
        ContractCheckId::AsyncCopyCancelNativeAttempt,
        ContractCheckId::AsyncCopyCancelReader,
        ContractCheckId::AsyncCopyCancelWriter,
        ContractCheckId::AsyncCopyCancelCommit,
    ] {
        let fixture = AsyncMemoryFixture::with_cancellation_limits(qubit_fs::metadata::FileSystemLimits::unknown());
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(run.requirements_satisfied(), "{id}: {:?}", run.failures());
            assert!(
                matches!(
                    run.report()
                        .checks()
                        .iter()
                        .find(|check| check.id() == id)
                        .map(|check| check.outcome()),
                    Some(super::super::ContractCheckOutcome::Passed)
                ),
                "{id} must execute its stage probe"
            );
        });
        assert!(fixture.is_empty(), "{id} must clean up its resources");
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_copy_cancellation_retains_each_probe_fault_at_every_stage() {
    let stages = [
        ContractCheckId::AsyncCopyCancelNativeAttempt,
        ContractCheckId::AsyncCopyCancelReader,
        ContractCheckId::AsyncCopyCancelWriter,
        ContractCheckId::AsyncCopyCancelCommit,
    ];
    let faults = [
        super::common::AsyncMemoryFault::CopyCancelAcknowledgeFails,
        super::common::AsyncMemoryFault::CopyCancelDisarmFails,
    ];
    for id in stages {
        for fault in faults {
            let fixture = AsyncMemoryFixture::with_fault(fault);
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert!(!run.failures().is_empty(), "{fault:?} must fail {id}");
                assert!(run.failures().iter().all(|failure| failure.check() == Some(id)));
            });
            assert!(fixture.is_empty(), "{fault:?} at {id} must clean up resources");
        }
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_write_cancellation_covers_every_supported_stage() {
    for id in [
        ContractCheckId::WriteCancelOpen,
        ContractCheckId::WriteCancelWrite,
        ContractCheckId::WriteCancelFlush,
        ContractCheckId::WriteCancelCommit,
    ] {
        let fixture = AsyncMemoryFixture::new();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(run.requirements_satisfied(), "{id}: {:?}", run.failures());
        });
        assert!(fixture.is_empty(), "{id} must clean up its resources");
    }
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_write_cancellation_retains_each_probe_fault_at_every_stage() {
    let stages = [
        ContractCheckId::WriteCancelOpen,
        ContractCheckId::WriteCancelWrite,
        ContractCheckId::WriteCancelFlush,
        ContractCheckId::WriteCancelCommit,
    ];
    let faults = [
        super::common::AsyncMemoryFault::WriteCancelObserveFails,
        super::common::AsyncMemoryFault::WriteCancelAcknowledgeFails,
        super::common::AsyncMemoryFault::WriteCancelAcceptedBytesFails,
        super::common::AsyncMemoryFault::WriteCancelDisarmFails,
    ];
    for id in stages {
        for fault in faults {
            let fixture = AsyncMemoryFixture::with_fault(fault);
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert!(!run.failures().is_empty(), "{fault:?} must fail {id}");
                assert!(run.failures().iter().all(|failure| failure.check() == Some(id)));
            });
            assert!(fixture.is_empty(), "{fault:?} at {id} must clean up resources");
        }
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
        super::common::AsyncMemoryFault::CopyCancelAcknowledgeFails,
        super::common::AsyncMemoryFault::CopyCancelDisarmFails,
        super::common::AsyncMemoryFault::CopyCancelFailsBeforeStage,
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
                super::common::shared_model::FixtureHook::ReadFile,
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

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_capable_profile_preserves_every_path_failure() {
    for id in ContractCheckId::ALL.iter().copied() {
        let baseline = AsyncMemoryFixture::with_all_capabilities();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&baseline);
            let _ = suite.run_check(id).await;
        });
        for call in 1..=baseline.path_call_count() {
            let fixture = AsyncMemoryFixture::with_path_error_call(call);
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert!(!run.failures().is_empty(), "{id}, path {call}");
                assert!(run.failures().iter().all(|failure| failure.check() == Some(id)));
            });
            assert!(fixture.is_empty(), "{id}, path {call} leaked resources");
        }
    }
}
