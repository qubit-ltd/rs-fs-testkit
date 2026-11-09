// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Tests for explicit cleanup of expected whole-file write rejections.

use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteFailureState;
use qubit_fs::write::WriteOptions;
use qubit_fs::write::WritePrecondition;

use crate::ContractCheckId;
use crate::FileSystemFixture;
use crate::FixtureSupport;
use crate::internal::finish_expected_write_failure::finish_expected_write_failure;
use crate::tests::common::MemoryFault;
use crate::tests::common::MemoryFixture;

#[test]
fn test_expected_write_rejection_cleans_or_rejects_recovery_outcomes() {
    for fault in [
        MemoryFault::ConditionalAbortFailsOnce,
        MemoryFault::ConditionalAbortPanicsOnce,
        MemoryFault::ConditionalAbortReportsPublished,
    ] {
        let fixture = MemoryFixture::with_fault(fault);
        let FixtureSupport::Supported(path) = fixture
            .seed_file("expected-write-conditional", b"original")
            .expect("seed expected-write target")
        else {
            panic!("memory fixture must support file seeding");
        };
        let failure = fixture
            .file_system()
            .write_all(
                &path,
                b"replacement",
                WriteOptions::default().with_precondition(WritePrecondition::IfAbsent),
            )
            .expect_err("IfAbsent must reject the seeded target");

        let result = finish_expected_write_failure(failure, ContractCheckId::WriteCreateConflict);
        assert!(result.is_err(), "{fault:?} cleanup must reject an unsafe outcome");
        let failure = result.expect_err("unsafe cleanup must remain a contract failure");
        assert_eq!(failure.check(), Some(ContractCheckId::WriteCreateConflict));
    }
}

#[test]
fn test_expected_write_rejection_without_recovery_needs_no_abort() {
    let fixture = MemoryFixture::with_fault(MemoryFault::WriteFails);
    let path = fixture.path("expected-write-open-failure").expect("valid test path");
    let failure = fixture
        .file_system()
        .write_all(&path, b"payload", WriteOptions::default())
        .expect_err("injected writer-open failure");

    assert!(failure.recovery().is_none());
    finish_expected_write_failure(failure, ContractCheckId::WriteCreateConflict)
        .expect("an open failure has no session to abort");
}

#[test]
fn test_expected_write_rejection_refuses_published_or_indeterminate_state() {
    for state in [WriteFailureState::Published, WriteFailureState::Indeterminate] {
        let fixture = MemoryFixture::with_fault(MemoryFault::WriteCommitState(state));
        let FixtureSupport::Supported(path) = fixture
            .seed_file("unsafe-write-state", b"original")
            .expect("seed expected-write target")
        else {
            panic!("memory fixture must support file seeding");
        };
        let failure = fixture
            .file_system()
            .write_all(
                &path,
                b"replacement",
                WriteOptions::default().with_disposition(WriteDisposition::CreateNew),
            )
            .expect_err("injected commit must fail with its configured publication state");

        let failure = finish_expected_write_failure(failure, ContractCheckId::WriteCreateConflict)
            .expect_err("published or indeterminate writes cannot be accepted as expected rejection");
        assert_eq!(failure.check(), Some(ContractCheckId::WriteCreateConflict));
        assert!(failure.message().contains("did not prove non-publication"));
    }
}

#[cfg(feature = "async")]
#[test]
fn test_async_expected_rejection_retains_a_published_abort_result() {
    use qubit_fs::write::WriteOptions;

    use crate::AsyncFileSystemFixture;
    use crate::internal::finish_expected_write_failure::finish_expected_async_write_failure;
    use crate::tests::common::AsyncMemoryFault;
    use crate::tests::common::AsyncMemoryFixture;
    use crate::tests::common::async_memory_file_system::run_controlled;

    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::ConditionalAbortReportsPublished);
    run_controlled(async {
        let FixtureSupport::Supported(path) = fixture
            .seed_file("expected-write-conditional", b"original")
            .await
            .expect("seed expected-write target")
        else {
            panic!("memory fixture must support file seeding");
        };
        let failure = crate::internal::execute_write::execute_write(
            fixture.file_system(),
            path,
            b"replacement".to_vec(),
            WriteOptions::default().with_precondition(WritePrecondition::IfAbsent),
        )
        .await
        .expect_err("IfAbsent must reject the seeded target");
        let mut failures = Vec::new();

        let failure = finish_expected_async_write_failure(failure, ContractCheckId::WriteCreateConflict, &mut failures)
            .await
            .expect_err("a published abort result is not proof of non-publication");
        assert!(
            failures.is_empty(),
            "completed cleanup must not be reported as cancelled"
        );
        assert!(failure.message().contains("did not confirm non-publication"));
    });
}
