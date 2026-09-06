mod common;

use std::panic::AssertUnwindSafe;

use common::MemoryFault;
use common::MemoryFixture;
use qubit_fs::error::FsErrorKind;
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs::path::Path;
use qubit_fs::read::ReadOptions;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureCase;

fn assert_panics_at<F>(run: F, check_id: &str)
where
    F: FnOnce(),
{
    let result = std::panic::catch_unwind(AssertUnwindSafe(run));
    let payload = result.expect_err("faulty contract fixture unexpectedly passed");
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|value| (*value).to_owned()))
        .unwrap_or_default();
    assert!(
        message.contains(check_id),
        "panic did not identify {check_id}: {message}"
    );
}

#[test]
fn test_sync_rejects_read_ignoring_stale_version() {
    let fixture = MemoryFixture::with_fault(MemoryFault::IgnoreReadIfMatch);
    assert_panics_at(
        || FileSystemContractSuite::new(&fixture).assert_contract(FileSystemContract::Read),
        "read/if-match-stale",
    );
}

#[test]
fn test_sync_rejects_read_ignoring_if_none_match() {
    let fixture = MemoryFixture::with_fault(MemoryFault::IgnoreReadIfNoneMatch);
    assert_panics_at(
        || FileSystemContractSuite::new(&fixture).assert_contract(FileSystemContract::Read),
        "read/if-none-match-current",
    );
}

#[test]
fn test_sync_rejects_write_ignoring_stale_version() {
    let fixture = MemoryFixture::with_fault(MemoryFault::IgnoreWriteIfMatch);
    assert_panics_at(
        || FileSystemContractSuite::new(&fixture).assert_contract(FileSystemContract::Write),
        "write/if-match",
    );
}

#[test]
fn test_sync_rejects_atomic_replace_preserving_old_bytes() {
    let fixture = MemoryFixture::with_fault(MemoryFault::AtomicReplaceKeepsOldBytes);
    assert_panics_at(
        || FileSystemContractSuite::new(&fixture).assert_contract(FileSystemContract::AtomicReplace),
        "write/atomic-replace-existing",
    );
}

#[test]
fn test_sync_rejects_durable_write_dropping_bytes() {
    let fixture = MemoryFixture::with_fault(MemoryFault::DurableWriteDropsBytes);
    assert_panics_at(
        || FileSystemContractSuite::new(&fixture).assert_contract(FileSystemContract::Write),
        "write/durable",
    );
}

#[test]
fn test_sync_rejects_checksum_corruption() {
    let fixture = MemoryFixture::with_fault(MemoryFault::ChecksumIgnoresCorruption);
    assert_panics_at(
        || FileSystemContractSuite::new(&fixture).assert_contract(FileSystemContract::Read),
        "read/checksum",
    );
}

#[test]
fn test_sync_honors_bounded_write_and_range_limits() {
    let limits = FileSystemLimits::unknown()
        .with_max_write_bytes(FileSystemLimit::Maximum(1))
        .with_max_read_range_bytes(FileSystemLimit::Maximum(1));
    let fixture = MemoryFixture::with_limits(limits);
    let path = Path::parse("/contract/limit-write").expect("limit path must parse");
    fixture
        .file_system()
        .write_all(&path, b"x", Default::default())
        .expect("one byte must fit Maximum(1)");
    let failure = fixture
        .file_system()
        .write_all(&path, b"xx", Default::default())
        .expect_err("second byte must exceed Maximum(1)");
    assert_eq!(failure.error().kind(), FsErrorKind::ResourceLimitExceeded);
    let read_failure = fixture
        .file_system()
        .open_reader(&path, ReadOptions::default().with_length(Some(2)))
        .expect_err("two-byte range must exceed Maximum(1)");
    assert_eq!(read_failure.kind(), FsErrorKind::ResourceLimitExceeded);
}

#[test]
fn test_sync_contracts_adapt_to_declared_limit_profiles() {
    let profiles = [
        FileSystemLimits::unknown().with_max_write_bytes(FileSystemLimit::Maximum(1)),
        FileSystemLimits::unknown().with_max_read_range_bytes(FileSystemLimit::Maximum(4)),
        FileSystemLimits::unknown()
            .with_max_write_bytes(FileSystemLimit::Unknown)
            .with_max_read_range_bytes(FileSystemLimit::Unknown),
        FileSystemLimits::unknown()
            .with_max_write_bytes(FileSystemLimit::NotApplicable)
            .with_max_read_range_bytes(FileSystemLimit::NotApplicable),
        FileSystemLimits::unknown()
            .with_max_write_bytes(FileSystemLimit::Unbounded)
            .with_max_read_range_bytes(FileSystemLimit::Unbounded),
        FileSystemLimits::unknown()
            .with_max_write_bytes(FileSystemLimit::Maximum(u64::MAX))
            .with_max_read_range_bytes(FileSystemLimit::Maximum(u64::MAX)),
    ];
    for limits in profiles {
        let fixture = MemoryFixture::with_limits(limits);
        let write_report =
            FileSystemContractSuite::new(&fixture).assert_contract_with_report(FileSystemContract::Write);
        write_report.assert_complete();
        let read_report = FileSystemContractSuite::new(&fixture).assert_contract_with_report(FileSystemContract::Read);
        read_report.assert_complete();
    }
}

#[test]
fn test_sync_conditional_case_unavailability_remains_unverified() {
    let fixture = MemoryFixture::with_conditional_case_unavailable(FixtureCase::ReadIfMatch);
    let report = FileSystemContractSuite::new(&fixture).assert_contract_with_report(FileSystemContract::Read);
    assert!(report.checks().iter().any(|check| {
        check.id() == "read/if-match-current" && matches!(check.outcome(), ContractCheckOutcome::Unverified { .. })
    }));
}

#[test]
fn test_sync_conditional_delete_case_unavailability_is_unverified() {
    let fixture = MemoryFixture::with_conditional_case_unavailable(FixtureCase::DeleteIfMatch);
    let report = FileSystemContractSuite::new(&fixture).assert_contract_with_report(FileSystemContract::Delete);
    assert!(report.checks().iter().any(|check| {
        check.id() == "delete/if-match" && matches!(check.outcome(), ContractCheckOutcome::Unverified { .. })
    }));
}

#[test]
fn test_sync_io_reports_are_complete_when_probes_run() {
    for contract in [
        FileSystemContract::Properties,
        FileSystemContract::Read,
        FileSystemContract::Write,
    ] {
        let fixture = MemoryFixture::with_all_capabilities();
        let report = FileSystemContractSuite::new(&fixture).assert_contract_with_report(contract);
        report.assert_complete();
    }
}
