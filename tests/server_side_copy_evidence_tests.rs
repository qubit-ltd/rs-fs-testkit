// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use common::server_side_copy_fixture::ServerSideCopyFixture;
pub use qubit_fs_testkit;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContractSuite;

fn check_sync_fault(name: &str, passed: bool, calls: usize) -> ContractCheckOutcome {
    let fixture = ServerSideCopyFixture::new(true, true);
    fixture.inject_server_side_fault(name);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::CopyServerSide);
    assert_eq!(run.requirements_satisfied(), passed, "fault: {name}");
    assert_eq!(fixture.server_side_calls(), calls, "fault: {name}");
    assert!(run.cleanup().completed(), "fault: {name}");
    assert!(fixture.is_empty(), "fault: {name}");
    if passed {
        assert!(run.failures().is_empty(), "fault: {name}");
    } else if ["read_unsupported", "read_unsupported_after_first", "exists_unsupported"].contains(&name) {
        assert!(run.failures().is_empty(), "fault: {name}");
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { .. }
        ));
    } else {
        assert_eq!(
            run.failures()[0].check(),
            Some(ContractCheckId::CopyServerSide),
            "fault: {name}"
        );
    }
    run.report().checks()[0].outcome().clone()
}

#[test]
fn test_one_byte_and_budget_boundary_sources_pass() {
    for name in ["one_byte_source", "max_source"] {
        assert_eq!(check_sync_fault(name, true, 1), ContractCheckOutcome::Passed);
    }
}

#[test]
fn test_empty_and_over_budget_sources_fail_before_copy() {
    for name in ["empty_source", "large_source"] {
        let _ = check_sync_fault(name, false, 0);
    }
}

#[test]
fn test_server_side_content_and_source_are_independently_verified() {
    for name in ["discard_content", "mutate_source"] {
        let _ = check_sync_fault(name, false, 1);
    }
}

#[test]
fn test_unsupported_observation_is_unverified_but_observation_error_fails() {
    for (name, calls) in [
        ("read_unsupported", 0),
        ("read_unsupported_after_first", 1),
        ("exists_unsupported", 0),
    ] {
        assert!(matches!(
            check_sync_fault(name, false, calls),
            ContractCheckOutcome::Unverified { .. }
        ));
    }
    let _ = check_sync_fault("read_error", false, 0);
    let _ = check_sync_fault("read_error_after_first", false, 1);
}

#[cfg(feature = "async")]
mod asynchronous {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use super::ContractCheckId;
    use super::ContractCheckOutcome;
    use super::common::async_memory_file_system::run_controlled;
    use super::common::server_side_copy_fixture::AsyncServerSideCopyFixture;

    fn check_fault(name: &'static str, passed: bool, calls: usize) -> ContractCheckOutcome {
        let fixture = AsyncServerSideCopyFixture::new(true, true);
        fixture.inject_server_side_fault(name);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::CopyServerSide).await;
            assert_eq!(run.requirements_satisfied(), passed, "fault: {name}");
            assert_eq!(fixture.server_side_calls(), calls, "fault: {name}");
            assert!(run.cleanup().completed(), "fault: {name}");
            assert!(fixture.is_empty(), "fault: {name}");
            if passed {
                assert!(run.failures().is_empty(), "fault: {name}");
            } else if ["read_unsupported", "read_unsupported_after_first", "exists_unsupported"].contains(&name) {
                assert!(run.failures().is_empty(), "fault: {name}");
                assert!(matches!(
                    run.report().checks()[0].outcome(),
                    ContractCheckOutcome::Unverified { .. }
                ));
            } else {
                assert_eq!(
                    run.failures()[0].check(),
                    Some(ContractCheckId::CopyServerSide),
                    "fault: {name}"
                );
            }
            run.report().checks()[0].outcome().clone()
        })
    }

    #[test]
    fn test_one_byte_and_budget_boundary_sources_pass() {
        for name in ["one_byte_source", "max_source"] {
            assert_eq!(check_fault(name, true, 1), ContractCheckOutcome::Passed);
        }
    }

    #[test]
    fn test_empty_and_over_budget_sources_fail_before_copy() {
        for name in ["empty_source", "large_source"] {
            let _ = check_fault(name, false, 0);
        }
    }

    #[test]
    fn test_server_side_content_and_source_are_independently_verified() {
        for name in ["discard_content", "mutate_source"] {
            let _ = check_fault(name, false, 1);
        }
    }

    #[test]
    fn test_unsupported_observation_is_unverified_but_observation_error_fails() {
        for (name, calls) in [
            ("read_unsupported", 0),
            ("read_unsupported_after_first", 1),
            ("exists_unsupported", 0),
        ] {
            assert!(matches!(
                check_fault(name, false, calls),
                ContractCheckOutcome::Unverified { .. }
            ));
        }
        let _ = check_fault("read_error", false, 0);
        let _ = check_fault("read_error_after_first", false, 1);
    }
}
