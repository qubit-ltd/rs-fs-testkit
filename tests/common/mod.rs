// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

#[cfg(feature = "async")]
pub(crate) mod async_memory_file_system;
pub(crate) mod check_matrix;
mod memory_file_system;
pub(crate) mod shared_model;

#[allow(unused)]
#[cfg(feature = "async")]
pub(crate) use async_memory_file_system::AsyncMemoryFault;
#[allow(unused)]
#[cfg(feature = "async")]
pub(crate) use async_memory_file_system::AsyncMemoryFixture;
#[allow(unused)]
pub use memory_file_system::MemoryFault;
#[allow(unused)]
pub use memory_file_system::MemoryFixture;

#[cfg(feature = "async")]
mod async_memory_write_cancellation_probe;
#[cfg(feature = "async")]
mod write_gate;

mod unavailable_scenario;
pub use unavailable_scenario::UnavailableScenario;

#[cfg(test)]
mod coverage_driver_tests {
    use super::MemoryFixture;
    use crate::qubit_fs_testkit::FileSystemContractSuite;

    /// Every integration-test target that uses the shared fixture exercises
    /// the complete synchronous contract suite in its own library instance.
    #[test]
    fn shared_fixture_runs_every_synchronous_contract() {
        let fixture = MemoryFixture::with_all_capabilities();
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_all();

        assert!(run.requirements_satisfied(), "{:?}", run.failures());
        assert!(fixture.is_empty(), "the complete suite must clean up");
    }

    #[test]
    fn shared_fixture_runs_every_synchronous_capability_profile() {
        use crate::qubit_fs_testkit::ContractCheckId;

        for id in ContractCheckId::ALL
            .iter()
            .copied()
            .filter(|id| id.supports_synchronous())
        {
            for (profile, fixture) in [
                ("all", MemoryFixture::with_all_capabilities()),
                ("no-operations", MemoryFixture::without_operation_capabilities()),
                ("core-only", MemoryFixture::without_optional_capabilities()),
                ("fallback", MemoryFixture::fallback_only()),
                ("read-only", MemoryFixture::read_only()),
            ] {
                let mut suite = FileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id);
                assert!(run.requirements_satisfied(), "{id}, {profile}: {:?}", run.failures());
                assert!(fixture.is_empty(), "{id}, {profile}: fixture leaked resources");
            }
        }
    }

    #[test]
    fn shared_fixture_runs_bounded_synchronous_limit_probes() {
        use qubit_fs::metadata::FileSystemLimit;
        use qubit_fs::metadata::FileSystemLimits;

        use crate::qubit_fs_testkit::ContractCheckId;

        let cases = [
            (
                ContractCheckId::PropertiesLimitPathAdmission,
                FileSystemLimits::unknown().with_max_path_text_bytes(FileSystemLimit::Maximum(8)),
            ),
            (
                ContractCheckId::PropertiesLimitComponentAdmission,
                FileSystemLimits::unknown().with_max_component_text_bytes(FileSystemLimit::Maximum(4)),
            ),
            (
                ContractCheckId::PropertiesLimitListPage,
                FileSystemLimits::unknown().with_max_list_page_entries(FileSystemLimit::Maximum(4)),
            ),
            (
                ContractCheckId::ReadRangeLimit,
                FileSystemLimits::unknown().with_max_read_range_bytes(FileSystemLimit::Maximum(4)),
            ),
            (
                ContractCheckId::WriteLimit,
                FileSystemLimits::unknown().with_max_write_bytes(FileSystemLimit::Maximum(4)),
            ),
            (
                ContractCheckId::ListPagination,
                FileSystemLimits::unknown().with_max_list_page_entries(FileSystemLimit::Maximum(2)),
            ),
        ];
        for (id, limits) in cases {
            let fixture = MemoryFixture::with_limits(limits);
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id);
            assert!(run.requirements_satisfied(), "{id}: {:?}", run.failures());
            assert!(fixture.is_empty(), "{id}: fixture leaked resources");
        }
    }

    #[test]
    fn shared_fixture_verifies_atomic_temp_persist_rejection() {
        use crate::qubit_fs_testkit::ContractCheckId;

        let fixture = MemoryFixture::without_atomic_temp_persist();
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempAtomic);
        assert!(run.requirements_satisfied(), "{:?}", run.failures());
        assert!(fixture.is_empty(), "atomic temp rejection must clean up");
    }

    #[test]
    fn shared_fixture_attributes_synchronous_temp_directory_failures() {
        use super::MemoryFault;
        use crate::qubit_fs_testkit::ContractCheckId;

        for id in [
            ContractCheckId::TempAtomic,
            ContractCheckId::TempDirectory,
            ContractCheckId::TempFile,
            ContractCheckId::TempRepeatedLifecycle,
        ] {
            let mut faults = vec![MemoryFault::TempCreationFails];
            if id != ContractCheckId::TempAtomic {
                faults.push(MemoryFault::TempCleanupFailsOnce);
            }
            if id == ContractCheckId::TempDirectory {
                faults.extend([
                    MemoryFault::TempKeepFails,
                    MemoryFault::TempPersistFails,
                    MemoryFault::TempIgnoresOptions,
                    MemoryFault::WrongPersistTarget,
                ]);
            }
            for fault in faults {
                let fixture = MemoryFixture::with_fault(fault);
                let mut suite = FileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id);
                assert!(!run.failures().is_empty(), "{fault:?} must fail {id}");
                assert!(
                    run.failures().iter().all(|failure| failure.check() == Some(id)),
                    "{fault:?} must be attributed to {id}"
                );
                suite.finish();
                assert!(fixture.is_empty(), "{fault:?} must clean up {id}");
            }
        }
    }

    #[test]
    fn shared_fixture_exercises_retained_failure_accessors() {
        use super::MemoryFault;
        use crate::qubit_fs_testkit::ContractCheckId;
        use crate::qubit_fs_testkit::ContractSource;

        let fixture = MemoryFixture::with_fault(MemoryFault::WriteCommitFailure);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::WriteBasic);
        let failure = run.failures().first().expect("write fault must be retained");
        assert_eq!(failure.check(), Some(ContractCheckId::WriteBasic));
        assert!(failure.take_panic_payload().is_none());
        let source = std::error::Error::source(failure)
            .and_then(|source| source.downcast_ref::<ContractSource>())
            .expect("write recovery source must be retained");
        assert!(source.take().is_some());
        assert!(source.take().is_none());
    }

    #[test]
    fn shared_fixture_faults_are_rejected_by_their_synchronous_check() {
        use super::check_matrix::sync_fault_cases;
        use crate::qubit_fs_testkit::ContractCheckId;

        for case in sync_fault_cases() {
            let id = ContractCheckId::ALL
                .iter()
                .copied()
                .find(|id| id.as_str() == case.check_id)
                .expect("fault matrix check must be registered");
            let fixture = MemoryFixture::with_fault(case.fault);
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id);
            assert!(
                !run.requirements_satisfied()
                    && (run.failures().iter().any(|failure| failure.check() == Some(id))
                        || run.report().checks().iter().any(|check| {
                            check.id() == id
                                && matches!(
                                    check.outcome(),
                                    crate::qubit_fs_testkit::ContractCheckOutcome::Failed { .. }
                                )
                        })),
                "{} must reject {:?}; outcome={:?}, failures={:?}",
                case.check_id,
                case.fault,
                run.report().checks().first().map(|check| check.outcome()),
                run.failures(),
            );
        }
    }

    /// Injects every fixture setup and observation error into the check that
    /// consumes it, so error attribution is covered in each test target.
    #[test]
    fn shared_fixture_errors_reach_every_synchronous_check() {
        use super::shared_model::FixtureHook;
        use crate::qubit_fs_testkit::ContractCheckId;

        let hooks = [
            FixtureHook::Snapshot,
            FixtureHook::ReadFile,
            FixtureHook::WriteFile,
            FixtureHook::ResourceVersion,
            FixtureHook::StaleResourceVersion,
            FixtureHook::ChecksumFailureCase,
            FixtureHook::SeedSymlink,
            FixtureHook::CopyFastPathCase,
        ];
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
                assert!(!run.failures().is_empty(), "{id}, path call {call}");
                assert!(run.failures().iter().all(|failure| failure.check() == Some(id)), "{id}");
            }
            for call in 1..=baseline.seed_file_call_count() {
                let fixture = MemoryFixture::with_seed_error_calls(Some(call), None);
                let mut suite = FileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id);
                assert!(!run.failures().is_empty(), "{id}, file seed call {call}");
                assert!(run.failures().iter().all(|failure| failure.check() == Some(id)), "{id}");
            }
            for call in 1..=baseline.seed_directory_call_count() {
                let fixture = MemoryFixture::with_seed_error_calls(None, Some(call));
                let mut suite = FileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id);
                assert!(!run.failures().is_empty(), "{id}, directory seed call {call}");
                assert!(run.failures().iter().all(|failure| failure.check() == Some(id)), "{id}");
            }
            for call in 1..=baseline.exists_call_count() {
                let fixture = MemoryFixture::with_exists_error_call(call);
                let mut suite = FileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id);
                assert!(!run.failures().is_empty(), "{id}, exists call {call}");
                assert!(run.failures().iter().all(|failure| failure.check() == Some(id)), "{id}");
            }
            if matches!(id, ContractCheckId::TempFile | ContractCheckId::TempDirectory) {
                for call in 1..=baseline.temp_creation_call_count() {
                    let fixture = MemoryFixture::with_temp_creation_error_call(call);
                    let mut suite = FileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id);
                    assert!(!run.failures().is_empty(), "{id}, temp creation call {call}");
                }
            }
            for hook in hooks {
                for call in 1..=baseline.fixture_hook_call_count(hook) {
                    let fixture = MemoryFixture::with_fixture_hook_error(hook, call);
                    let mut suite = FileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id);
                    assert!(!run.failures().is_empty(), "{id}, {hook:?} call {call}");
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
    fn shared_fixture_runs_every_asynchronous_contract() {
        use super::AsyncMemoryFixture;
        use super::async_memory_file_system::run_controlled;
        use crate::qubit_fs_testkit::AsyncFileSystemContractSuite;

        let fixture = AsyncMemoryFixture::with_all_capabilities();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_all().await;
            assert!(run.requirements_satisfied(), "{:?}", run.failures());
        });
        assert!(fixture.is_empty(), "the complete suite must clean up");
    }

    #[cfg(feature = "async")]
    #[test]
    fn shared_fixture_runs_every_asynchronous_capability_profile() {
        use super::AsyncMemoryFixture;
        use super::async_memory_file_system::run_controlled;
        use crate::qubit_fs_testkit::AsyncFileSystemContractSuite;
        use crate::qubit_fs_testkit::ContractCheckId;

        for id in ContractCheckId::ALL.iter().copied() {
            for (profile, fixture) in [
                ("all", AsyncMemoryFixture::with_all_capabilities()),
                ("no-operations", AsyncMemoryFixture::without_operation_capabilities()),
                ("core-only", AsyncMemoryFixture::without_optional_capabilities()),
                ("fallback", AsyncMemoryFixture::fallback_only()),
            ] {
                run_controlled(async {
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert!(run.requirements_satisfied(), "{id}, {profile}: {:?}", run.failures());
                });
                assert!(fixture.is_empty(), "{id}, {profile}: fixture leaked resources");
            }
        }
    }

    #[cfg(feature = "async")]
    #[test]
    fn shared_fixture_runs_bounded_asynchronous_limit_probes() {
        use qubit_fs::metadata::FileSystemLimit;
        use qubit_fs::metadata::FileSystemLimits;

        use super::AsyncMemoryFixture;
        use super::async_memory_file_system::run_controlled;
        use crate::qubit_fs_testkit::AsyncFileSystemContractSuite;
        use crate::qubit_fs_testkit::ContractCheckId;

        let cases = [
            (
                ContractCheckId::PropertiesLimitPathAdmission,
                FileSystemLimits::unknown().with_max_path_text_bytes(FileSystemLimit::Maximum(8)),
            ),
            (
                ContractCheckId::PropertiesLimitComponentAdmission,
                FileSystemLimits::unknown().with_max_component_text_bytes(FileSystemLimit::Maximum(4)),
            ),
            (
                ContractCheckId::PropertiesLimitListPage,
                FileSystemLimits::unknown().with_max_list_page_entries(FileSystemLimit::Maximum(4)),
            ),
            (
                ContractCheckId::ReadRangeLimit,
                FileSystemLimits::unknown().with_max_read_range_bytes(FileSystemLimit::Maximum(4)),
            ),
            (
                ContractCheckId::WriteLimit,
                FileSystemLimits::unknown().with_max_write_bytes(FileSystemLimit::Maximum(4)),
            ),
            (
                ContractCheckId::ListPagination,
                FileSystemLimits::unknown().with_max_list_page_entries(FileSystemLimit::Maximum(2)),
            ),
        ];
        for (id, limits) in cases {
            let fixture = AsyncMemoryFixture::with_limits(limits);
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
    fn shared_fixture_verifies_asynchronous_atomic_temp_persist_rejection() {
        use super::AsyncMemoryFixture;
        use super::async_memory_file_system::run_controlled;
        use crate::qubit_fs_testkit::AsyncFileSystemContractSuite;
        use crate::qubit_fs_testkit::ContractCheckId;

        let fixture = AsyncMemoryFixture::without_atomic_temp_persist();
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::TempAtomic).await;
            assert!(run.requirements_satisfied(), "{:?}", run.failures());
        });
        assert!(fixture.is_empty(), "atomic temp rejection must clean up");
    }

    #[cfg(feature = "async")]
    #[test]
    fn shared_fixture_attributes_asynchronous_temp_directory_failures() {
        use super::AsyncMemoryFixture;
        use super::async_memory_file_system::run_controlled;
        use crate::qubit_fs_testkit::AsyncFileSystemContractSuite;
        use crate::qubit_fs_testkit::ContractCheckId;

        for id in [
            ContractCheckId::TempAtomic,
            ContractCheckId::TempDirectory,
            ContractCheckId::TempFile,
            ContractCheckId::TempRepeatedLifecycle,
        ] {
            let mut faults = vec![super::AsyncMemoryFault::TempCreationFails];
            if id != ContractCheckId::TempAtomic {
                faults.push(super::AsyncMemoryFault::TempCleanupFailsOnce);
            }
            if id == ContractCheckId::TempDirectory {
                faults.extend([
                    super::AsyncMemoryFault::TempKeepFails,
                    super::AsyncMemoryFault::TempPersistFails,
                    super::AsyncMemoryFault::TempIgnoresOptions,
                    super::AsyncMemoryFault::TempCleanupNoOp,
                ]);
            }
            for fault in faults {
                let fixture = AsyncMemoryFixture::with_fault(fault);
                run_controlled(async {
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert!(!run.failures().is_empty(), "{fault:?} must fail {id}");
                    assert!(
                        run.failures().iter().all(|failure| failure.check() == Some(id)),
                        "{fault:?} must be attributed to {id}"
                    );
                    suite.finish().await;
                });
                assert!(fixture.is_empty(), "{fault:?} must clean up {id}");
            }
        }
    }

    #[cfg(feature = "async")]
    #[test]
    fn shared_fixture_reads_async_write_fixture_options() {
        use qubit_fs::path::Path;
        use qubit_fs::write::WriteDisposition;
        use qubit_fs::write::WriteOptions;

        use crate::qubit_fs_testkit::AsyncWriteFixtureCase;

        let case = AsyncWriteFixtureCase::new(
            Path::parse("/coverage-write-case").expect("valid test path"),
            b"payload".to_vec(),
            WriteOptions::default().with_disposition(WriteDisposition::CreateNew),
        );
        assert_eq!(case.options().disposition(), WriteDisposition::CreateNew);
    }

    #[cfg(feature = "async")]
    #[test]
    fn shared_fixture_attributes_async_cancellation_admission_errors() {
        use super::AsyncMemoryFixture;
        use super::async_memory_file_system::run_controlled;
        use crate::qubit_fs_testkit::AsyncFileSystemContractSuite;
        use crate::qubit_fs_testkit::ContractCheckId;

        let copy_fixture = AsyncMemoryFixture::new().with_invalid_probe_path("source");
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&copy_fixture);
            let run = suite.run_check(ContractCheckId::AsyncCopyCancelReader).await;
            assert_eq!(run.failures()[0].check(), Some(ContractCheckId::AsyncCopyCancelReader));
            assert!(
                run.failures()[0]
                    .message()
                    .contains("copy cancellation request admission failed")
            );
        });

        let write_fixture = AsyncMemoryFixture::new().with_invalid_probe_path("Open");
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&write_fixture);
            let run = suite.run_check(ContractCheckId::WriteCancelOpen).await;
            assert_eq!(run.failures()[0].check(), Some(ContractCheckId::WriteCancelOpen));
            assert!(
                run.failures()[0]
                    .message()
                    .contains("write cancellation request admission failed")
            );
        });
    }

    #[cfg(feature = "async")]
    #[test]
    fn shared_fixture_faults_are_rejected_by_their_asynchronous_check() {
        use super::AsyncMemoryFixture;
        use super::async_memory_file_system::run_controlled;
        use super::check_matrix::async_fault_cases;
        use crate::qubit_fs_testkit::AsyncFileSystemContractSuite;
        use crate::qubit_fs_testkit::ContractCheckId;

        for case in async_fault_cases() {
            let id = ContractCheckId::ALL
                .iter()
                .copied()
                .find(|id| id.as_str() == case.check_id)
                .expect("fault matrix check must be registered");
            let fixture = AsyncMemoryFixture::with_fault(case.fault);
            run_controlled(async {
                let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                let run = suite.run_check(id).await;
                assert!(
                    !run.requirements_satisfied()
                        && (run.failures().iter().any(|failure| failure.check() == Some(id))
                            || run.report().checks().iter().any(|check| {
                                check.id() == id
                                    && matches!(
                                        check.outcome(),
                                        crate::qubit_fs_testkit::ContractCheckOutcome::Failed { .. }
                                    )
                            })),
                    "{} must reject {:?}; outcome={:?}, failures={:?}",
                    case.check_id,
                    case.fault,
                    run.report().checks().first().map(|check| check.outcome()),
                    run.failures(),
                );
            });
        }
    }

    #[cfg(feature = "async")]
    #[test]
    fn shared_fixture_errors_reach_every_asynchronous_check() {
        use super::AsyncMemoryFixture;
        use super::async_memory_file_system::run_controlled;
        use super::shared_model::FixtureHook;
        use crate::qubit_fs_testkit::AsyncFileSystemContractSuite;
        use crate::qubit_fs_testkit::ContractCheckId;

        let hooks = [
            FixtureHook::Snapshot,
            FixtureHook::ReadFile,
            FixtureHook::WriteFile,
            FixtureHook::ResourceVersion,
            FixtureHook::StaleResourceVersion,
            FixtureHook::ChecksumFailureCase,
            FixtureHook::SeedSymlink,
            FixtureHook::CopyFastPathCase,
        ];
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
                    assert!(!run.failures().is_empty(), "{id}, path call {call}");
                    assert!(run.failures().iter().all(|failure| failure.check() == Some(id)), "{id}");
                });
            }
            for call in 1..=baseline.seed_file_call_count() {
                let fixture = AsyncMemoryFixture::with_seed_error_calls(Some(call), None);
                run_controlled(async {
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert!(!run.failures().is_empty(), "{id}, file seed call {call}");
                    assert!(run.failures().iter().all(|failure| failure.check() == Some(id)), "{id}");
                });
            }
            for call in 1..=baseline.seed_directory_call_count() {
                let fixture = AsyncMemoryFixture::with_seed_error_calls(None, Some(call));
                run_controlled(async {
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert!(!run.failures().is_empty(), "{id}, directory seed call {call}");
                    assert!(run.failures().iter().all(|failure| failure.check() == Some(id)), "{id}");
                });
            }
            for call in 1..=baseline.exists_call_count() {
                let fixture = AsyncMemoryFixture::with_exists_error_call(call);
                run_controlled(async {
                    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                    let run = suite.run_check(id).await;
                    assert!(!run.failures().is_empty(), "{id}, exists call {call}");
                    assert!(run.failures().iter().all(|failure| failure.check() == Some(id)), "{id}");
                });
            }
            if matches!(id, ContractCheckId::TempFile | ContractCheckId::TempDirectory) {
                for call in 1..=baseline.temp_creation_call_count() {
                    let fixture = AsyncMemoryFixture::with_temp_creation_error_call(call);
                    run_controlled(async {
                        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                        let run = suite.run_check(id).await;
                        assert!(!run.failures().is_empty(), "{id}, temp creation call {call}");
                    });
                }
            }
            for hook in hooks {
                for call in 1..=baseline.fixture_hook_call_count(hook) {
                    let fixture = AsyncMemoryFixture::with_fixture_hook_error(hook, call);
                    run_controlled(async {
                        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
                        let run = suite.run_check(id).await;
                        assert!(!run.failures().is_empty(), "{id}, {hook:?} call {call}");
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
