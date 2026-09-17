// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Original failure snapshots and rejected sessions survive contract reporting.

mod common;
use common::MemoryFault;
use common::MemoryFixture;
use qubit_fs::error::OpenFailure;
use qubit_fs::error::OpenFailureStage;
use qubit_fs::write::RejectedWriter;
use qubit_fs::write::WriteAllFailure;
use qubit_fs::write::WriteFailureState;
use qubit_fs::write::WriterRecovery;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractSource;
use qubit_fs_testkit::FileSystemContractSuite;

/// A real facade failure crosses the report wrapper without losing frozen
/// facts.
#[test]
fn test_contract_preserves_all_commit_snapshots_after_recovery() {
    for state in [
        WriteFailureState::RetryableNotPublished,
        WriteFailureState::NotPublished,
        WriteFailureState::Published,
        WriteFailureState::Indeterminate,
    ] {
        let fixture = MemoryFixture::with_fault(MemoryFault::WriteCommitState(state));
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(ContractCheckId::WriteBasic);
        let source = std::error::Error::source(&run.failures()[0])
            .expect("source")
            .downcast_ref::<ContractSource>()
            .expect("owned source");
        let mut failure = source
            .take()
            .expect("owned error")
            .downcast::<WriteAllFailure>()
            .expect("whole-file failure");
        assert_eq!(failure.state(), state);
        let bytes = failure.written_bytes();
        assert_eq!(bytes, b"written".len() as u64);
        let Some(WriterRecovery::Opened(writer)) = failure.recovery_mut() else {
            panic!("validated writer")
        };
        let _outcome = writer.abort().expect("explicit cleanup");
        assert_eq!(failure.state(), state);
        assert_eq!(failure.written_bytes(), bytes);
        drop(failure.take_recovery().expect("transfer ownership"));
        assert_eq!(failure.state(), state);
        assert_eq!(failure.written_bytes(), bytes);
        assert!(!run.requirements_satisfied());
    }
}

/// An opening error with a non-Sync isolated session remains recoverable.
#[test]
fn test_contract_report_retains_rejected_writer_open_failure() {
    let fixture = MemoryFixture::with_fault(MemoryFault::InvalidWriterIdentity);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::WriteAbort);
    let source = std::error::Error::source(&run.failures()[0])
        .expect("source")
        .downcast_ref::<ContractSource>()
        .expect("owned source");
    let mut failure = source
        .take()
        .expect("owned error")
        .downcast::<OpenFailure<RejectedWriter>>()
        .expect("original opening failure");
    assert_eq!(failure.stage(), OpenFailureStage::OutcomeValidation);
    let _outcome = failure
        .recovery_mut()
        .expect("isolated session")
        .abort()
        .expect("explicit cleanup");
    assert_eq!(failure.stage(), OpenFailureStage::OutcomeValidation);
    assert!(source.take().is_none());
    assert!(!run.requirements_satisfied());
}

/// Expected precondition errors cannot hide cleanup failures or their sessions.
#[test]
fn test_conditional_cleanup_failure_preserves_sync_recovery() {
    use qubit_fs::error::FsErrorKind;
    use qubit_fs::write::WriteAbortOutcome;
    use qubit_fs_testkit::ContractWriterFailure;
    for id in [ContractCheckId::WriteIfAbsent, ContractCheckId::WriteIfMatch] {
        let fixture = MemoryFixture::with_fault(MemoryFault::ConditionalAbortFailsOnce);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);
        assert!(!run.requirements_satisfied());
        let source = std::error::Error::source(&run.failures()[0])
            .expect("source")
            .downcast_ref::<ContractSource>()
            .expect("owned source");
        let mut retained = source
            .take()
            .expect("owned failure")
            .downcast::<ContractWriterFailure<WriteAllFailure>>()
            .expect("original failure and cleanup error");
        assert_eq!(retained.writer().error().kind(), FsErrorKind::PreconditionFailed);
        let state = retained.writer().state();
        let bytes = retained.writer().written_bytes();
        let Some(WriterRecovery::Opened(writer)) = retained.writer_mut().recovery_mut() else {
            panic!("retained writer")
        };
        assert_eq!(writer.abort().expect("explicit retry"), WriteAbortOutcome::NotPublished);
        assert_eq!(retained.writer().state(), state);
        assert_eq!(retained.writer().written_bytes(), bytes);
    }
}
