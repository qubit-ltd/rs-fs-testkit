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

fn assert_list_kind_fault(fault: MemoryFault, id: ContractCheckId) {
    let fixture = MemoryFixture::with_fault(fault);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(id);
    assert!(!run.requirements_satisfied(), "{id} must detect {fault:?}");
    assert!(
        run.failures()
            .iter()
            .any(|failure| failure.check() == Some(id))
    );
    assert!(run.report().checks().iter().any(|check| {
        check.id() == id && matches!(check.outcome(), ContractCheckOutcome::Failed { .. })
    }));
    assert!(fixture.is_empty(), "failed {id} must clean seeded entries");
}

#[test]
fn test_list_basic_and_prefix_reject_wrong_independently_seeded_kinds() {
    assert_list_kind_fault(MemoryFault::ListFileAsDirectory, ContractCheckId::ListBasic);
    assert_list_kind_fault(
        MemoryFault::ListDirectoryAsFile,
        ContractCheckId::ListPrefix,
    );
}

#[test]
fn test_list_basic_and_prefix_accept_correct_hierarchical_kinds() {
    for id in [ContractCheckId::ListBasic, ContractCheckId::ListPrefix] {
        let fixture = MemoryFixture::new();
        let mut suite = FileSystemContractSuite::new(&fixture);
        suite.run_check(id).assert_satisfied();
        assert!(fixture.is_empty());
    }
}

#[test]
fn test_list_accepts_flat_object_kind_for_seeded_file() {
    let fixture = MemoryFixture::with_fault(MemoryFault::ListFileAsObject);
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite
        .run_check(ContractCheckId::ListBasic)
        .assert_satisfied();
    assert!(fixture.is_empty());
}

#[cfg(feature = "async")]
#[test]
fn test_async_list_basic_and_prefix_reject_wrong_independently_seeded_kinds() {
    use common::AsyncMemoryFault;
    use common::AsyncMemoryFixture;
    use common::async_memory_file_system::run_controlled;
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    run_controlled(async {
        for (fault, id) in [
            (
                AsyncMemoryFault::ListFileAsDirectory,
                ContractCheckId::ListBasic,
            ),
            (
                AsyncMemoryFault::ListDirectoryAsFile,
                ContractCheckId::ListPrefix,
            ),
        ] {
            let fixture = AsyncMemoryFixture::with_fault(fault);
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_check(id).await;
            assert!(!run.requirements_satisfied(), "{id} must detect {fault:?}");
            assert!(
                run.failures()
                    .iter()
                    .any(|failure| failure.check() == Some(id))
            );
            assert!(run.report().checks().iter().any(|check| {
                check.id() == id && matches!(check.outcome(), ContractCheckOutcome::Failed { .. })
            }));
            assert!(fixture.is_empty(), "failed {id} must clean seeded entries");
        }
    });
}

#[cfg(feature = "async")]
#[test]
fn test_async_list_basic_and_prefix_accept_correct_hierarchical_kinds() {
    use common::AsyncMemoryFixture;
    use common::async_memory_file_system::run_controlled;
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    run_controlled(async {
        for id in [ContractCheckId::ListBasic, ContractCheckId::ListPrefix] {
            let fixture = AsyncMemoryFixture::new();
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            suite.run_check(id).await.assert_satisfied();
            assert!(fixture.is_empty());
        }
    });
}

#[cfg(feature = "async")]
#[test]
fn test_async_list_accepts_flat_object_kind_for_seeded_file() {
    use common::AsyncMemoryFault;
    use common::AsyncMemoryFixture;
    use common::async_memory_file_system::run_controlled;
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    run_controlled(async {
        let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::ListFileAsObject);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        suite
            .run_check(ContractCheckId::ListBasic)
            .await
            .assert_satisfied();
        assert!(fixture.is_empty());
    });
}
