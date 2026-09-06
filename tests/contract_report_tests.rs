//! Contract report identity and completeness regressions.

mod common;

use common::MemoryFixture;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;

#[test]
fn empty_report_is_complete_and_assertion_is_idempotent() {
    let report = qubit_fs_testkit::ContractReport::default();
    assert!(report.is_complete());
    report.assert_complete();
}

#[test]
fn report_entries_have_phase_metadata_and_unique_ids() {
    let fixture = MemoryFixture::new();
    let report = FileSystemContractSuite::new(&fixture).assert_contract_with_report(FileSystemContract::Properties);
    assert!(!report.checks().is_empty());
    assert!(
        report
            .checks()
            .iter()
            .all(|check| { check.phase() == FileSystemContract::Properties && check.is_required() })
    );
    let mut ids = report.checks().iter().map(|check| check.id()).collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), report.checks().len());
    assert!(!report.is_complete());
}

#[test]
fn unsupported_optional_probes_are_distinguished_from_unverified_checks() {
    let fixture = MemoryFixture::without_optional_capabilities();
    let report = FileSystemContractSuite::new(&fixture).assert_contract_with_report(FileSystemContract::Read);
    assert!(
        report
            .checks()
            .iter()
            .filter(|check| check.phase() == FileSystemContract::Read)
            .all(|check| {
                check.is_required() || !matches!(check.outcome(), ContractCheckOutcome::Unverified { .. })
            })
    );
}
