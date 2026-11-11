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
use qubit_fs::path::Path;
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureFuture;
use qubit_fs_testkit::FixtureResult;

use self::common::AsyncMemoryFixture;
use self::common::async_memory_file_system::run_controlled;

struct TeardownFailureFixture {
    inner: AsyncMemoryFixture,
}

impl AsyncFileSystemFixture for TeardownFailureFixture {
    fn file_system(&self) -> &AsyncFileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn teardown(&self) -> FixtureFuture<'_, ()> {
        Box::pin(async { Err(FixtureError::new("injected async teardown failure")) })
    }
}

#[test]
fn test_async_report_assertion_does_not_hide_failed_teardown() {
    let fixture = TeardownFailureFixture {
        inner: AsyncMemoryFixture::new(),
    };
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(FileSystemContract::ErrorContext).await;

        run.report().assert_satisfied();
        assert!(!run.requirements_satisfied());
        assert!(!run.cleanup().failures().is_empty());
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run.assert_satisfied();
            }))
            .is_err()
        );
        assert!(std::error::Error::source(&run.cleanup().failures()[0]).is_some());
    });
}
