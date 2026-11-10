// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

pub use ::qubit_fs_testkit;

mod common;

use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContractSuite;

use self::common::MemoryFixture;

#[test]
fn test_read_range_runs_with_sufficient_and_insufficient_caller_budgets() {
    for maximum in [1, 4] {
        let limits = FileSystemLimits::unknown().with_max_read_range_bytes(FileSystemLimit::Maximum(maximum));
        let fixture = MemoryFixture::with_fault_and_limits(common::MemoryFault::None, limits);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::ReadRange);
        assert!(run.requirements_satisfied(), "maximum={maximum}: {:?}", run.failures());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Passed
        ));
        let window_requests = fixture
            .read_requests()
            .into_iter()
            .filter(|request| *request == (Some(5), Some(maximum)))
            .count();
        assert_eq!(
            window_requests, 3,
            "maximum={maximum}: baseline, accepted-budget and rejected-budget requests must use the same window"
        );
    }
}

#[cfg(feature = "async")]
#[test]
fn test_async_read_range_runs_with_sufficient_and_insufficient_caller_budgets() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    run_controlled(async {
        for maximum in [1, 4] {
            let limits = FileSystemLimits::unknown().with_max_read_range_bytes(FileSystemLimit::Maximum(maximum));
            let fixture = AsyncMemoryFixture::with_fault_and_limits(AsyncMemoryFault::None, limits);
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(ContractCheckId::ReadRange).await;
            assert!(run.requirements_satisfied(), "maximum={maximum}: {:?}", run.failures());
            assert!(
                matches!(run.report().checks()[0].outcome(), ContractCheckOutcome::Passed),
                "maximum={maximum}: {:?}",
                run.report().checks()[0].outcome()
            );
            let window_requests = fixture
                .read_requests()
                .into_iter()
                .filter(|request| *request == (Some(5), Some(maximum)))
                .count();
            assert_eq!(
                window_requests, 3,
                "maximum={maximum}: baseline, accepted-budget and rejected-budget requests must use the same window"
            );
        }
    });
}
