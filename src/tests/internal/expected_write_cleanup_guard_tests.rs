// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Tests for retaining an expected-write failure when cleanup is abandoned.

#[cfg(feature = "async")]
#[test]
fn test_drop_records_the_original_failure_when_cleanup_is_cancelled() {
    use qubit_fs::write::WriteDisposition;
    use qubit_fs::write::WriteOptions;

    use crate::AsyncFileSystemFixture;
    use crate::ContractCheckId;
    use crate::FixtureSupport;
    use crate::internal::execute_write::execute_write;
    use crate::internal::expected_write_cleanup_guard::ExpectedWriteCleanupGuard;
    use crate::tests::common::AsyncMemoryFixture;
    use crate::tests::common::async_memory_file_system::run_controlled;

    let fixture = AsyncMemoryFixture::new();
    run_controlled(async {
        let FixtureSupport::Supported(path) = fixture
            .seed_file("cancelled-expected-write", b"original")
            .await
            .expect("seed expected-write target")
        else {
            panic!("memory fixture must support file seeding");
        };
        let failure = execute_write(
            fixture.file_system(),
            path,
            b"replacement".to_vec(),
            WriteOptions::default().with_disposition(WriteDisposition::CreateNew),
        )
        .await
        .expect_err("CreateNew must reject the seeded target");
        let mut failures = Vec::new();

        drop(ExpectedWriteCleanupGuard::new(
            failure,
            &mut failures,
            ContractCheckId::WriteCreateConflict,
        ));

        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].check(), Some(ContractCheckId::WriteCreateConflict));
        assert!(failures[0].message().contains("cleanup was cancelled"));
        assert!(std::error::Error::source(&failures[0]).is_some());
    });
}
