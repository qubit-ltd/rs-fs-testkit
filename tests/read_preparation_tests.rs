// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Read scenarios retain precise ordinary failures and independent setup.

mod common;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;

use self::common::MemoryFault;
use self::common::MemoryFixture;
/// Deliberately rejected read requests must be distinguishable from reads.
#[test]
fn test_read_rejection_evidence_is_explicit() {
    let fixture = MemoryFixture::with_all_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Read);
    for id in [
        ContractCheckId::ReadIfMatchStale,
        ContractCheckId::ReadIfNoneMatchCurrent,
        ContractCheckId::ReadChecksumCorruption,
    ] {
        let check = run
            .report()
            .checks()
            .iter()
            .find(|check| check.id() == id)
            .expect("registered read check");
        assert!(
            matches!(check.outcome(), ContractCheckOutcome::RejectedAsExpected),
            "{id}: {:?}",
            check.outcome()
        );
    }
    assert!(run.requirements_satisfied());
}

/// Async read rejection has the same report semantics as synchronous read.
#[cfg(feature = "async")]
#[test]
fn test_async_read_rejection_evidence_is_explicit() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::new();
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(FileSystemContract::Read).await;
        for id in [
            ContractCheckId::ReadIfMatchStale,
            ContractCheckId::ReadIfNoneMatchCurrent,
            ContractCheckId::ReadChecksumCorruption,
        ] {
            let check = run
                .report()
                .checks()
                .iter()
                .find(|check| check.id() == id)
                .expect("registered read check");
            assert!(
                matches!(check.outcome(), ContractCheckOutcome::RejectedAsExpected),
                "{id}: {:?}",
                check.outcome()
            );
        }
        assert!(run.requirements_satisfied());
    });
}

/// Ignoring a stale condition is a failed check, not an unattributed panic.
#[test]
fn test_stale_read_failure_retains_check_identity() {
    let fixture = MemoryFixture::with_fault(MemoryFault::IgnoreReadIfMatch);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Read);
    assert_eq!(run.failures()[0].check(), Some(ContractCheckId::ReadIfMatchStale));
    assert!(
        run.report()
            .checks()
            .iter()
            .any(|check| check.id() == ContractCheckId::ReadIfMatchStale
                && matches!(check.outcome(), ContractCheckOutcome::Failed { .. }))
    );
    assert!(fixture.is_empty());
}

#[path = "common/selective_read_fixture.rs"]
mod selective_read_fixture;

/// A missing basic setup must not prevent an independently prepared range
/// check.
#[test]
fn test_read_checks_prepare_their_own_scenarios() {
    let fixture = selective_read_fixture::SelectiveReadFixture::new();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Read);
    assert!(!run.requirements_satisfied());
    assert!(run.failures().is_empty());
    assert!(
        run.report()
            .checks()
            .iter()
            .any(|check| check.id() == ContractCheckId::ReadBasic
                && matches!(check.outcome(), ContractCheckOutcome::Unverified { .. }))
    );
    assert!(run.report().checks().iter().any(
        |check| check.id() == ContractCheckId::ReadRange && matches!(check.outcome(), ContractCheckOutcome::Passed)
    ));
    assert!(run.cleanup().completed());
}

/// Both I/O drivers preserve the same stale-precondition failure identity.
#[cfg(feature = "async")]
#[test]
fn test_async_stale_read_failure_retains_check_identity() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::IgnoreReadIfMatch);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(FileSystemContract::Read).await;
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::ReadIfMatchStale));
        assert!(
            run.report()
                .checks()
                .iter()
                .any(|check| check.id() == ContractCheckId::ReadIfMatchStale
                    && matches!(check.outcome(), ContractCheckOutcome::Failed { .. }))
        );
        assert!(fixture.is_empty());
    });
}
