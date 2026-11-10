// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

pub use ::qubit_fs_testkit;
use common::MemoryFault;
use common::MemoryFixture;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContractSuite;

#[test]
fn test_hierarchical_sync_listing_detects_depth_faults_and_cleans_up() {
    for id in [ContractCheckId::ListBasic, ContractCheckId::ListPrefix] {
        let fixture = MemoryFixture::new();
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        run.assert_satisfied();
        assert!(fixture.is_empty());
    }

    for (fault, id) in [
        (MemoryFault::ListAlwaysRecursive, ContractCheckId::ListBasic),
        (MemoryFault::ListAlwaysDirectChildren, ContractCheckId::ListPrefix),
    ] {
        let fixture = MemoryFixture::with_fault(fault);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        assert!(!run.requirements_satisfied(), "{id} must detect {fault:?}");
        assert!(run.failures().iter().any(|failure| failure.check() == Some(id)));
        assert!(
            run.report()
                .checks()
                .iter()
                .any(|check| { check.id() == id && matches!(check.outcome(), ContractCheckOutcome::Failed { .. }) })
        );
        assert!(fixture.is_empty(), "failed {id} must clean seeded entries");
    }

    for (fixture, id) in [
        (
            MemoryFixture::with_unavailable_seed_calls(Some(1), None),
            ContractCheckId::ListBasic,
        ),
        (
            MemoryFixture::with_unavailable_seed_calls(Some(2), None),
            ContractCheckId::ListPrefix,
        ),
    ] {
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { .. }
        ));
        assert!(fixture.is_empty(), "partially prepared directory sets must be cleaned");
    }
    let fixture = MemoryFixture::with_seed_error_calls(Some(1), Some(2));
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::ListPrefix);
    assert!(!run.requirements_satisfied());
    assert!(
        run.failures()
            .iter()
            .any(|failure| failure.check() == Some(ContractCheckId::ListPrefix))
    );
    assert!(fixture.is_empty(), "directory seed failure must clean earlier seeds");
}

#[cfg(feature = "async")]
#[test]
fn test_hierarchical_async_listing_detects_depth_faults_and_cleans_up() {
    use common::AsyncMemoryFault;
    use common::AsyncMemoryFixture;
    use common::async_memory_file_system::run_controlled;
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    run_controlled(async {
        for id in [ContractCheckId::ListBasic, ContractCheckId::ListPrefix] {
            let fixture = AsyncMemoryFixture::new();
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            run.assert_satisfied();
            assert!(fixture.is_empty());
        }

        for (fault, id) in [
            (AsyncMemoryFault::ListAlwaysRecursive, ContractCheckId::ListBasic),
            (AsyncMemoryFault::ListAlwaysDirectChildren, ContractCheckId::ListPrefix),
        ] {
            let fixture = AsyncMemoryFixture::with_fault(fault);
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(!run.requirements_satisfied(), "{id} must detect {fault:?}");
            assert!(run.failures().iter().any(|failure| failure.check() == Some(id)));
            assert!(
                run.report().checks().iter().any(|check| {
                    check.id() == id && matches!(check.outcome(), ContractCheckOutcome::Failed { .. })
                })
            );
            assert!(fixture.is_empty(), "failed {id} must clean seeded entries");
        }
        for (fixture, id) in [
            (
                AsyncMemoryFixture::with_unavailable_seed_calls(Some(1), None),
                ContractCheckId::ListBasic,
            ),
            (
                AsyncMemoryFixture::with_unavailable_seed_calls(Some(2), None),
                ContractCheckId::ListPrefix,
            ),
        ] {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(matches!(
                run.report().checks()[0].outcome(),
                ContractCheckOutcome::Unverified { .. }
            ));
            assert!(fixture.is_empty(), "partially prepared directory sets must be cleaned");
        }
        let fixture = AsyncMemoryFixture::with_seed_error_calls(Some(1), Some(2));
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::ListPrefix).await;
        assert!(!run.requirements_satisfied());
        assert!(
            run.failures()
                .iter()
                .any(|failure| failure.check() == Some(ContractCheckId::ListPrefix))
        );
        assert!(fixture.is_empty(), "directory seed failure must clean earlier seeds");
    });
}
