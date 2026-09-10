//! Shared InMemory and explicit real-service recovery observations.

use std::future::Future;
use std::pin::Pin;

use qubit_fs::write::AsyncWriterRecovery;
use qubit_fs::write::WriteAbortOutcome;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteFailureState;
use qubit_fs::write::WriteOptions;
use qubit_fs_s3_contract::TestStage;
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::FixtureSupport;

use super::Fixture;

/// Runs conditional and cancellation checks with independent SDK observations.
/// The caller must always invoke fixture teardown, including after a panic.
pub async fn verify(fixture: &Fixture) {
    for id in [
        ContractCheckId::WriteIfAbsent,
        ContractCheckId::WriteIfMatch,
        ContractCheckId::WriteCancelOpen,
        ContractCheckId::WriteCancelWrite,
        ContractCheckId::WriteCancelFlush,
        ContractCheckId::WriteCancelCommit,
    ] {
        AsyncFileSystemContractSuite::new(fixture)
            .run_check(id)
            .await
            .assert_satisfied();
    }
    let path = fixture.path("sdk-result-suppressed").expect("unique fixture path");
    fixture.control.arm(TestStage::AfterPutBeforeResult);
    let before_puts = fixture.control.put_calls();
    let before_aborts = fixture.control.abort_calls();
    let mut operation = fixture
        .filesystem
        .begin_write_all(
            path.clone(),
            b"published evidence".to_vec(),
            WriteOptions::default().with_disposition(WriteDisposition::CreateNew),
        )
        .expect("operation");
    let mut execute = Box::pin(operation.execute());
    reach(execute.as_mut(), fixture).await;
    let observed = fixture
        .read_file(&path)
        .await
        .expect("independent SDK GET before result delivery");
    assert!(matches!(observed, FixtureSupport::Supported(bytes) if bytes == b"published evidence"));
    drop(execute);
    fixture.control.release();
    let state = operation.state();
    let confirmed = operation.written_bytes();
    let Some(AsyncWriterRecovery::Opened(mut writer)) = operation.take_recovery() else {
        panic!("retained writer")
    };
    fixture.control.fail_next_abort();
    assert!(writer.abort_async().await.is_err());
    assert_eq!(
        writer.abort_async().await.expect("explicit retry"),
        WriteAbortOutcome::Published
    );
    assert_eq!(operation.state(), state);
    assert_eq!(operation.written_bytes(), confirmed);
    assert_eq!(
        operation.execute().await.expect_err("no reexecution").state(),
        WriteFailureState::Indeterminate
    );
    assert_eq!(fixture.control.put_calls() - before_puts, 1);
    assert_eq!(fixture.control.abort_calls() - before_aborts, 2);
    let observed = fixture
        .read_file(&path)
        .await
        .expect("independent SDK GET after recovery");
    assert!(matches!(observed, FixtureSupport::Supported(bytes) if bytes == b"published evidence"));
}

/// Polls the actual SDK operation until adapter acknowledgement, without
/// sleeps.
async fn reach<F: Future>(mut future: Pin<&mut F>, fixture: &Fixture) {
    std::future::poll_fn(|cx| {
        assert!(
            future.as_mut().poll(cx).is_pending(),
            "operation completed before result suppression"
        );
        fixture.control.poll_reached(cx)
    })
    .await;
}
