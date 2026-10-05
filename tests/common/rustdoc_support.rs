// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// Deterministic fixtures used only by executable Rustdoc examples.

#[path = "fs_rustdoc_support.rs"]
mod fs_rustdoc_support;

#[cfg(feature = "async")]
pub use fs_rustdoc_support::poll_support;
pub use fs_rustdoc_support::rustdoc_provider;
use qubit_fs::FileSystem;
use qubit_fs::path::Path;

use crate::qubit_fs_testkit::FileSystemFixture;
use crate::qubit_fs_testkit::FixtureError;
use crate::qubit_fs_testkit::FixtureResult;

/// Minimal synchronous fixture backed by the shared in-memory rustdoc provider.
pub struct RustdocSyncFixture {
    file_system: FileSystem,
}

impl RustdocSyncFixture {
    /// Creates an isolated fixture for Rustdoc contract-suite examples.
    pub fn new() -> Self {
        Self {
            file_system: rustdoc_provider::filesystem(),
        }
    }
}

impl FileSystemFixture for RustdocSyncFixture {
    fn file_system(&self) -> &FileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/rustdoc/{relative}"))
            .map_err(|error| FixtureError::with_source("invalid rustdoc path", error))
    }

    fn teardown(&self) -> FixtureResult<()> {
        Ok(())
    }
}

#[cfg(feature = "async")]
mod asynchronous {
    use qubit_fs::AsyncFileSystem;
    use qubit_fs::path::Path;

    use super::fs_rustdoc_support::async_filesystem;
    use crate::qubit_fs_testkit::AsyncFileSystemFixture;
    use crate::qubit_fs_testkit::FixtureError;
    use crate::qubit_fs_testkit::FixtureFuture;
    use crate::qubit_fs_testkit::FixtureResult;

    /// Minimal asynchronous fixture backed by the shared recording provider.
    pub struct RustdocAsyncFixture {
        file_system: AsyncFileSystem,
    }

    impl RustdocAsyncFixture {
        /// Creates an isolated asynchronous fixture for Rustdoc examples.
        pub fn new() -> Self {
            Self {
                file_system: async_filesystem(),
            }
        }
    }

    impl AsyncFileSystemFixture for RustdocAsyncFixture {
        fn file_system(&self) -> &AsyncFileSystem {
            &self.file_system
        }

        fn path(&self, relative: &str) -> FixtureResult<Path> {
            Path::parse(&format!("/rustdoc/{relative}"))
                .map_err(|error| FixtureError::with_source("invalid rustdoc path", error))
        }

        fn teardown(&self) -> FixtureFuture<'_, ()> {
            Box::pin(async { Ok(()) })
        }
    }
}

#[cfg(feature = "async")]
pub use asynchronous::RustdocAsyncFixture;

#[cfg(test)]
mod suite_unit_build_coverage_tests {
    #[cfg(feature = "async")]
    use super::super::AsyncFileSystemContractSuite;
    #[cfg(feature = "async")]
    use super::super::AsyncFileSystemFixture;
    use super::super::ContractCheckId;
    use super::super::FileSystemContract;
    use super::super::FileSystemContractSuite;
    use super::super::FileSystemFixture;
    #[cfg(feature = "async")]
    use super::super::common::AsyncMemoryFixture;
    use super::super::common::MemoryFixture;
    #[cfg(feature = "async")]
    use super::super::common::async_memory_file_system::run_controlled;
    #[cfg(feature = "async")]
    use super::super::common::check_matrix::async_fault_cases;
    use super::super::common::check_matrix::sync_fault_cases;
    #[cfg(feature = "async")]
    use super::super::rustdoc_support::poll_support;

    #[test]
    fn every_synchronous_check_executes_in_the_library_test_build() {
        let fixture = MemoryFixture::with_all_capabilities();
        let mut suite = FileSystemContractSuite::new(&fixture);
        suite.run_all().assert_satisfied();
        assert!(fixture.is_empty(), "the full synchronous suite must clean up");
    }

    #[test]
    fn synchronous_suite_handles_default_unsupported_fixture_hooks() {
        let fixture = super::super::rustdoc_support::RustdocSyncFixture::new();
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
    fn synchronous_fixture_defaults_return_explicit_unsupported_evidence() {
        use super::super::CopyScenario;
        use super::super::DeleteScenario;
        use super::super::ReadScenario;
        use super::super::WriteScenario;

        let fixture = super::super::rustdoc_support::RustdocSyncFixture::new();
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
            fixture.copy_fast_path_case(qubit_fs::copy::CopyMethod::ServerSide),
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
    fn synchronous_advertised_capability_requires_seed_support() {
        let fixture = MemoryFixture::with_unavailable_seed_calls(None, Some(1));
        let mut suite = FileSystemContractSuite::new(&fixture);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            suite.required_seed("required-seed", b"payload", "coverage probe");
        }));
        assert!(result.is_err());
    }

    #[test]
    fn synchronous_copy_checks_reject_unadvertised_guarantees() {
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
    fn synchronous_server_side_copy_reports_unavailable_fixture_evidence() {
        let fixture =
            MemoryFixture::with_conditional_case_unavailable(super::super::common::UnavailableScenario::Capability(
                qubit_fs::metadata::FileSystemCapability::ServerSideCopy,
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
    fn synchronous_server_side_copy_rejects_an_unadvertised_capability() {
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
    fn synchronous_temp_directory_paths_retain_creation_and_cleanup_failures() {
        for (fault, id) in [
            (
                super::super::common::MemoryFault::TempCreationFails,
                ContractCheckId::TempDirectory,
            ),
            (
                super::super::common::MemoryFault::TempCleanupFailsOnce,
                ContractCheckId::TempDirectory,
            ),
            (
                super::super::common::MemoryFault::TempKeepFails,
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
    fn temporary_directories_reject_atomic_persist_when_not_advertised() {
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
    fn synchronous_core_profile_preserves_path_failures() {
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
    fn synchronous_checks_execute_under_each_capability_profile() {
        for id in ContractCheckId::ALL
            .iter()
            .copied()
            .filter(|id| id.supports_synchronous())
        {
            for profile in 0_u8..5 {
                let fixture = match profile {
                    0 => MemoryFixture::with_all_capabilities(),
                    1 => MemoryFixture::without_operation_capabilities(),
                    2 => MemoryFixture::without_optional_capabilities(),
                    3 => MemoryFixture::fallback_only(),
                    _ => MemoryFixture::read_only(),
                };
                let mut suite = FileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id);
                assert_eq!(run.report().checks().len(), 1, "{id}, profile {profile}");
                assert_eq!(run.report().checks()[0].id(), id, "{id}, profile {profile}");
                assert!(
                    run.requirements_satisfied(),
                    "{id}, profile {profile}: {:?}",
                    run.failures()
                );
                assert!(fixture.is_empty(), "{id}, profile {profile}: leaked resources");
            }
        }
    }

    #[test]
    fn synchronous_provider_faults_execute_in_the_library_test_build() {
        for case in sync_fault_cases() {
            for contract in FileSystemContract::ALL {
                let fixture = MemoryFixture::with_fault(case.fault);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    FileSystemContractSuite::new(&fixture)
                        .run_contract(contract)
                        .assert_satisfied();
                }));
                let _ = result;
            }
        }
    }

    #[test]
    fn synchronous_focused_faults_and_fixture_errors_execute_in_the_library_test_build() {
        use super::super::common::shared_model::FixtureHook;

        for case in sync_fault_cases() {
            let Some(id) = ContractCheckId::ALL
                .iter()
                .copied()
                .find(|id| id.as_str() == case.check_id)
            else {
                continue;
            };
            let fixture = MemoryFixture::with_fault(case.fault);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                FileSystemContractSuite::new(&fixture).run_check(id).assert_satisfied();
            }));
        }

        for id in ContractCheckId::ALL
            .iter()
            .copied()
            .filter(|id| id.supports_synchronous())
        {
            let baseline = MemoryFixture::with_all_capabilities();
            let mut baseline_suite = FileSystemContractSuite::new(&baseline);
            let _ = baseline_suite.run_check(id);
            if matches!(id, ContractCheckId::TempFile | ContractCheckId::TempDirectory) {
                for call in 1..=baseline.temp_creation_call_count() {
                    let fixture = MemoryFixture::with_temp_creation_error_call(call);
                    let mut suite = FileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id);
                    assert!(!run.failures().is_empty(), "{id}, temp create call {call}");
                }
            }
            for call in 1..=baseline.path_call_count() {
                let fixture = MemoryFixture::with_path_error_call(call);
                let mut suite = FileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id);
                assert!(!run.failures().is_empty(), "{id}, path {call}");
                assert!(run.failures().iter().all(|failure| failure.check() == Some(id)), "{id}");
            }
            for call in 1..=baseline.seed_file_call_count() {
                let fixture = MemoryFixture::with_seed_error_calls(Some(call), None);
                let mut suite = FileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id);
                assert!(!run.failures().is_empty(), "{id}, file seed {call}");
                assert!(run.failures().iter().all(|failure| failure.check() == Some(id)), "{id}");
            }
            for call in 1..=baseline.seed_directory_call_count() {
                let fixture = MemoryFixture::with_seed_error_calls(None, Some(call));
                let mut suite = FileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id);
                assert!(!run.failures().is_empty(), "{id}, directory seed {call}");
                assert!(run.failures().iter().all(|failure| failure.check() == Some(id)), "{id}");
            }
            for call in 1..=baseline.exists_call_count() {
                let fixture = MemoryFixture::with_exists_error_call(call);
                let mut suite = FileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id);
                assert!(!run.failures().is_empty(), "{id}, exists {call}");
                assert!(run.failures().iter().all(|failure| failure.check() == Some(id)), "{id}");
            }
            for hook in [
                FixtureHook::Snapshot,
                FixtureHook::ReadFile,
                FixtureHook::WriteFile,
                FixtureHook::ResourceVersion,
                FixtureHook::StaleResourceVersion,
                FixtureHook::ChecksumFailureCase,
                FixtureHook::SeedSymlink,
                FixtureHook::CopyFastPathCase,
            ] {
                for call in 1..=baseline.fixture_hook_call_count(hook) {
                    let fixture = MemoryFixture::with_fixture_hook_error(hook, call);
                    let mut suite = FileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id);
                    assert!(!run.failures().is_empty(), "{id}, {hook:?}, call {call}");
                    assert!(
                        run.failures().iter().all(|failure| failure.check() == Some(id)),
                        "{id}, {hook:?}"
                    );
                }
            }
        }
    }

    #[cfg(feature = "async")]
    #[test]
    fn every_asynchronous_check_executes_in_the_library_test_build() {
        let fixture = AsyncMemoryFixture::with_all_capabilities();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            suite.run_all().await.assert_satisfied();
        });
        assert!(fixture.is_empty(), "the full asynchronous suite must clean up");
    }

    #[cfg(feature = "async")]
    #[test]
    fn asynchronous_suite_handles_default_unsupported_fixture_hooks() {
        let fixture = super::super::rustdoc_support::RustdocAsyncFixture::new();
        assert!(matches!(
            poll_support::ready(fixture.seed_file("coverage-seed", b"payload")),
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
    fn asynchronous_fixture_defaults_return_explicit_unsupported_evidence() {
        use super::super::CopyScenario;
        use super::super::DeleteScenario;
        use super::super::ReadScenario;
        use super::super::WriteScenario;

        let fixture = super::super::rustdoc_support::RustdocAsyncFixture::new();
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
                fixture
                    .copy_fast_path_case(qubit_fs::copy::CopyMethod::ServerSide)
                    .await,
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
    fn asynchronous_advertised_capability_requires_seed_support() {
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
    fn asynchronous_copy_checks_reject_unadvertised_guarantees() {
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
    fn asynchronous_server_side_copy_reports_unavailable_fixture_evidence() {
        let fixture = AsyncMemoryFixture::with_conditional_case_unavailable(
            super::super::common::UnavailableScenario::Capability(
                qubit_fs::metadata::FileSystemCapability::ServerSideCopy,
            ),
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
    fn asynchronous_server_side_copy_rejects_an_unadvertised_capability() {
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
    fn asynchronous_temp_directory_paths_retain_creation_and_cleanup_failures() {
        for (fault, id) in [
            (
                super::super::common::AsyncMemoryFault::TempCreationFails,
                ContractCheckId::TempDirectory,
            ),
            (
                super::super::common::AsyncMemoryFault::TempCleanupFailsOnce,
                ContractCheckId::TempDirectory,
            ),
            (
                super::super::common::AsyncMemoryFault::TempKeepFails,
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
    fn asynchronous_copy_cancellation_attributes_admission_path_errors() {
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
    fn asynchronous_copy_cancellation_retains_probe_failures() {
        for fault in [
            super::super::common::AsyncMemoryFault::CopyCancelAcknowledgeFails,
            super::super::common::AsyncMemoryFault::CopyCancelDisarmFails,
            super::super::common::AsyncMemoryFault::CopyCancelFailsBeforeStage,
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
                    super::super::common::shared_model::FixtureHook::ReadFile,
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
    fn asynchronous_optional_profile_preserves_path_failures() {
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
    fn asynchronous_checks_execute_under_each_capability_profile() {
        for id in ContractCheckId::ALL.iter().copied() {
            for profile in 0_u8..4 {
                let fixture = match profile {
                    0 => AsyncMemoryFixture::with_all_capabilities(),
                    1 => AsyncMemoryFixture::without_operation_capabilities(),
                    2 => AsyncMemoryFixture::without_optional_capabilities(),
                    _ => AsyncMemoryFixture::fallback_only(),
                };
                run_controlled(async {
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert_eq!(run.report().checks().len(), 1, "{id}, profile {profile}");
                    assert_eq!(run.report().checks()[0].id(), id, "{id}, profile {profile}");
                    assert!(
                        run.requirements_satisfied(),
                        "{id}, profile {profile}: {:?}",
                        run.failures()
                    );
                });
                assert!(fixture.is_empty(), "{id}, profile {profile}: leaked resources");
            }
        }
    }

    #[cfg(feature = "async")]
    #[test]
    fn asynchronous_provider_faults_execute_in_the_library_test_build() {
        for case in async_fault_cases() {
            for contract in FileSystemContract::ALL {
                let fixture = AsyncMemoryFixture::with_fault(case.fault);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_controlled(async {
                        AsyncFileSystemContractSuite::new(&fixture)
                            .run_contract(contract)
                            .await
                            .assert_satisfied();
                    });
                }));
                let _ = result;
            }
        }
    }

    #[cfg(feature = "async")]
    #[test]
    fn asynchronous_focused_faults_and_fixture_errors_execute_in_the_library_test_build() {
        use super::super::common::shared_model::FixtureHook;

        for case in async_fault_cases() {
            let Some(id) = ContractCheckId::ALL
                .iter()
                .copied()
                .find(|id| id.as_str() == case.check_id)
            else {
                continue;
            };
            let fixture = AsyncMemoryFixture::with_fault(case.fault);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_controlled(async {
                    AsyncFileSystemContractSuite::new(&fixture)
                        .run_check(id)
                        .await
                        .assert_satisfied();
                });
            }));
        }

        for id in ContractCheckId::ALL.iter().copied() {
            let baseline = AsyncMemoryFixture::with_all_capabilities();
            run_controlled(async {
                let mut baseline_suite = AsyncFileSystemContractSuite::new(&baseline);
                let _ = baseline_suite.run_check(id).await;
            });
            if matches!(id, ContractCheckId::TempFile | ContractCheckId::TempDirectory) {
                for call in 1..=baseline.temp_creation_call_count() {
                    let fixture = AsyncMemoryFixture::with_temp_creation_error_call(call);
                    run_controlled(async {
                        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                        let run = suite.run_check(id).await;
                        assert!(!run.failures().is_empty(), "{id}, temp create call {call}");
                    });
                }
            }
            for call in 1..=baseline.path_call_count() {
                let fixture = AsyncMemoryFixture::with_path_error_call(call);
                run_controlled(async {
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert!(!run.failures().is_empty(), "{id}, path {call}");
                    assert!(run.failures().iter().all(|failure| failure.check() == Some(id)), "{id}");
                });
            }
            for call in 1..=baseline.seed_file_call_count() {
                let fixture = AsyncMemoryFixture::with_seed_error_calls(Some(call), None);
                run_controlled(async {
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert!(!run.failures().is_empty(), "{id}, file seed {call}");
                    assert!(run.failures().iter().all(|failure| failure.check() == Some(id)), "{id}");
                });
            }
            for call in 1..=baseline.seed_directory_call_count() {
                let fixture = AsyncMemoryFixture::with_seed_error_calls(None, Some(call));
                run_controlled(async {
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert!(!run.failures().is_empty(), "{id}, directory seed {call}");
                    assert!(run.failures().iter().all(|failure| failure.check() == Some(id)), "{id}");
                });
            }
            for call in 1..=baseline.exists_call_count() {
                let fixture = AsyncMemoryFixture::with_exists_error_call(call);
                run_controlled(async {
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert!(!run.failures().is_empty(), "{id}, exists {call}");
                    assert!(run.failures().iter().all(|failure| failure.check() == Some(id)), "{id}");
                });
            }
            for hook in [
                FixtureHook::Snapshot,
                FixtureHook::ReadFile,
                FixtureHook::WriteFile,
                FixtureHook::ResourceVersion,
                FixtureHook::StaleResourceVersion,
                FixtureHook::ChecksumFailureCase,
                FixtureHook::SeedSymlink,
                FixtureHook::CopyFastPathCase,
            ] {
                for call in 1..=baseline.fixture_hook_call_count(hook) {
                    let fixture = AsyncMemoryFixture::with_fixture_hook_error(hook, call);
                    run_controlled(async {
                        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                        let run = suite.run_check(id).await;
                        assert!(!run.failures().is_empty(), "{id}, {hook:?}, call {call}");
                        assert!(
                            run.failures().iter().all(|failure| failure.check() == Some(id)),
                            "{id}, {hook:?}"
                        );
                    });
                }
            }
        }
    }
}
