use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;

use self::common::MemoryFixture;
mod common;

/// A provider exposing only the stream fallback still passes the applicable
/// copy contract and records the native overwrite boundary precisely.
#[test]
fn test_sync_accepts_fallback_only_copy() {
    let fixture = MemoryFixture::fallback_only();
    let report = FileSystemContractSuite::new(&fixture)
        .run_contract(FileSystemContract::Copy)
        .report()
        .clone();
    report.assert_satisfied();
    assert!(report.checks().iter().any(|check| {
        check.id().as_str() == "copy/fallback-overwrite-rejected"
            && matches!(check.outcome(), ContractCheckOutcome::RejectedAsExpected)
    }));
    assert!(fixture.is_empty(), "fallback copy contract leaked resources");
}

/// Copy reports the missing write capability for a read-only provider.
#[test]
fn test_sync_read_only_reports_missing_write_for_copy() {
    let fixture = MemoryFixture::read_only();
    FileSystemContractSuite::new(&fixture)
        .run_contract(FileSystemContract::Copy)
        .assert_satisfied();
}
