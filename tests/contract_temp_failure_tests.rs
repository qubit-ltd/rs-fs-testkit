// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Failed persistence keeps the resource available through the borrowed report.

mod common;

use qubit_fs::temp::PersistFailure;
use qubit_fs::temp::PersistFailureState;
use qubit_fs::temp::TempResourceState;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractSource;
use qubit_fs_testkit::ContractTempFailure;

/// A failed required-atomic persist retains both its snapshot and cleanup
/// handle.
#[test]
fn test_sync_temp_failure_retains_cleanup_handle() {
    use qubit_fs::temp::TempFile;
    use qubit_fs_testkit::FileSystemContractSuite;

    use self::common::MemoryFault;
    use self::common::MemoryFixture;
    let fixture = MemoryFixture::with_fault(MemoryFault::AtomicTempPersistNonAtomic);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::TempAtomic);
    let failure = &run.failures()[0];
    assert_eq!(failure.check(), Some(ContractCheckId::TempAtomic));
    assert!(failure.take_panic_payload().is_none());
    let source = std::error::Error::source(failure)
        .expect("source")
        .downcast_ref::<ContractSource>()
        .expect("retained resource source");
    let mut retained = source
        .take()
        .expect("resource ownership")
        .downcast::<ContractTempFailure<TempFile>>()
        .expect("temporary file failure");
    assert_eq!(retained.resource().state(), TempResourceState::CleanupRequired);
    assert_eq!(
        retained
            .error()
            .downcast_ref::<PersistFailure>()
            .expect("original failure")
            .state(),
        PersistFailureState::PublishedSourceRetained
    );
    retained.resource_mut().cleanup().expect("explicit recovery cleanup");
    let (error, resource) = retained.into_parts();
    assert!(error.downcast_ref::<PersistFailure>().is_some());
    assert_eq!(resource.state(), TempResourceState::Cleaned);
    assert!(source.take().is_none());
    assert!(!run.requirements_satisfied());
}

/// Async persistence transfers cleanup responsibility without async Drop.
#[cfg(feature = "async")]
#[test]
fn test_async_temp_failure_retains_cleanup_handle() {
    use qubit_fs::temp::AsyncTempFile;
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::AtomicTempPersistNonAtomic);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::TempAtomic).await;
        let failure = &run.failures()[0];
        assert_eq!(failure.check(), Some(ContractCheckId::TempAtomic));
        assert!(failure.take_panic_payload().is_none());
        let source = std::error::Error::source(failure)
            .expect("source")
            .downcast_ref::<ContractSource>()
            .expect("retained resource source");
        let mut retained = source
            .take()
            .expect("resource ownership")
            .downcast::<ContractTempFailure<AsyncTempFile>>()
            .expect("temporary file failure");
        assert_eq!(retained.resource().state(), TempResourceState::CleanupRequired);
        assert_eq!(
            retained
                .error()
                .downcast_ref::<PersistFailure>()
                .expect("original failure")
                .state(),
            PersistFailureState::PublishedSourceRetained
        );
        retained
            .resource_mut()
            .cleanup()
            .await
            .expect("explicit recovery cleanup");
        assert_eq!(retained.resource().state(), TempResourceState::Cleaned);
        assert!(source.take().is_none());
        assert!(!run.requirements_satisfied());
    });
}
