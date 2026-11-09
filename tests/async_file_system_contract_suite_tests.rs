// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![cfg(feature = "async")]

pub use ::qubit_fs_testkit;

mod common;

use qubit_fs::AsyncFileSystem;
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs::path::Path;
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureFuture;
use qubit_fs_testkit::FixtureResult;

use self::common::AsyncMemoryFixture;
use self::common::async_memory_file_system::run_controlled;

struct TeardownFixture {
    inner: AsyncMemoryFixture,
    panic: bool,
}

impl AsyncFileSystemFixture for TeardownFixture {
    fn file_system(&self) -> &AsyncFileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn teardown(&self) -> FixtureFuture<'_, ()> {
        Box::pin(async move {
            if self.panic {
                panic!("injected async fixture teardown panic");
            }
            Err(FixtureError::new("injected async fixture teardown failure"))
        })
    }
}

#[test]
fn test_async_finish_capture_retains_teardown_error_and_panic() {
    for panic in [false, true] {
        let fixture = TeardownFixture {
            inner: AsyncMemoryFixture::new(),
            panic,
        };
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_contract(FileSystemContract::ErrorContext).await;
            assert!(!run.cleanup().failures().is_empty());
            assert!(!run.requirements_satisfied());
        });
    }
}

#[test]
fn test_async_suite_records_capability_limits_and_fixture_boundaries() {
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;

    run_controlled(async {
        let fixture = AsyncMemoryFixture::without_optional_capabilities();
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::AppendBasic).await;
        run.assert_satisfied();
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::RejectedAsExpected
        ));

        let fixture = AsyncMemoryFixture::tree_copy_without_directory_creation();
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::DirectoryCreate).await;
        run.assert_satisfied();
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::RejectedAsExpected
        ));

        let fixture = AsyncMemoryFixture::with_limits(
            FileSystemLimits::unknown().with_max_path_text_bytes(FileSystemLimit::Maximum(4)),
        );
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::PropertiesLimitPathAdmission).await;
        run.assert_satisfied();
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::RejectedAsExpected
        ));

        let fixture = AsyncMemoryFixture::with_limits(
            FileSystemLimits::unknown().with_max_component_text_bytes(FileSystemLimit::Maximum(4)),
        );
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite
            .run_check(ContractCheckId::PropertiesLimitComponentAdmission)
            .await;
        run.assert_satisfied();
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::RejectedAsExpected
        ));

        let fixture = AsyncMemoryFixture::with_unavailable_seed_calls(None, Some(1));
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::StatFileKind).await;
        assert!(run.failures().is_empty());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { .. }
        ));

        let fixture = AsyncMemoryFixture::with_unavailable_seed_calls(Some(1), None);
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::RepresentationEmpty).await;
        assert!(run.failures().is_empty());
        assert!(matches!(
            run.report().checks()[0].outcome(),
            ContractCheckOutcome::Unverified { .. }
        ));
    });
}
