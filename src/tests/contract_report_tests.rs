// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Tests for completeness and invalid transitions in contract reports.

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::contract_report::ContractReport;

#[test]
fn test_missing_catalog_entry_is_incomplete() {
    let mut report = ContractReport::new();
    report.expect(ContractCheckId::ReadBasic);
    report.record(ContractCheckId::WriteBasic, None, ContractCheckOutcome::Passed);
    assert!(!report.requirements_satisfied());
}

#[test]
fn test_unverified_catalog_entry_is_incomplete_until_recorded() {
    let mut report = ContractReport::new();
    report.expect(ContractCheckId::ReadBasic);
    report.record(
        ContractCheckId::ReadBasic,
        None,
        ContractCheckOutcome::Unverified {
            reason: "check not yet executed".to_owned(),
        },
    );

    assert!(!report.requirements_satisfied());
    let result = std::panic::catch_unwind(|| report.assert_satisfied());
    assert!(result.is_err(), "an unexecuted check must fail strict mode");
}

#[test]
fn test_skipped_optional_does_not_make_report_incomplete() {
    let mut report = ContractReport::new();
    report.expect(ContractCheckId::AsyncCopyCancelReader);
    report.record(
        ContractCheckId::AsyncCopyCancelReader,
        None,
        ContractCheckOutcome::SkippedOptional {
            reason: "probe unavailable".to_owned(),
        },
    );
    assert!(report.requirements_satisfied());
}

#[test]
fn test_optional_evidence_can_be_required_by_the_caller() {
    let mut report = ContractReport::new();
    let id = ContractCheckId::AsyncCopyCancelReader;
    report.expect(id);
    report.record(
        id,
        None,
        ContractCheckOutcome::SkippedOptional {
            reason: "no suspension gate".to_owned(),
        },
    );
    assert!(report.requirements_satisfied());
    assert!(!report.all_applicable_checks_verified());
    assert!(!report.requirements_satisfied_with(&[id]));
}

#[test]
fn test_failed_optional_probe_is_never_accepted() {
    let mut report = ContractReport::new();
    report.expect(ContractCheckId::AsyncCopyCancelReader);
    report.record(
        ContractCheckId::AsyncCopyCancelReader,
        None,
        ContractCheckOutcome::Failed {
            reason: "recovery writer was lost".to_owned(),
        },
    );
    assert!(!report.requirements_satisfied());
    assert!(!report.all_applicable_checks_verified());
}

#[test]
fn test_not_run_is_not_evidence() {
    let mut report = ContractReport::new();
    report.expect(ContractCheckId::ReadBasic);
    report.record(
        ContractCheckId::ReadBasic,
        None,
        ContractCheckOutcome::NotRun {
            reason: "earlier provider panic".to_owned(),
        },
    );
    assert!(!report.requirements_satisfied());
    assert!(!report.all_applicable_checks_verified());
}

#[test]
fn test_required_probe_must_belong_to_the_report() {
    let mut report = ContractReport::new();
    report.expect(ContractCheckId::ReadBasic);
    report.record(ContractCheckId::ReadBasic, None, ContractCheckOutcome::Passed);
    assert!(report.requirements_satisfied());
    assert!(!report.requirements_satisfied_with(&[ContractCheckId::WriteCancelOpen]));
}

#[test]
fn test_duplicate_completion_invalidates_report() {
    let mut report = ContractReport::new();
    report.expect(ContractCheckId::ReadBasic);
    report.record(ContractCheckId::ReadBasic, None, ContractCheckOutcome::Passed);
    report.record(ContractCheckId::ReadBasic, None, ContractCheckOutcome::Passed);
    assert!(!report.requirements_satisfied());
}

#[test]
fn test_unregistered_completion_invalidates_report() {
    let mut report = ContractReport::new();
    report.expect(ContractCheckId::ReadBasic);
    report.record(ContractCheckId::ReadBasic, None, ContractCheckOutcome::Passed);
    report.record(ContractCheckId::WriteBasic, None, ContractCheckOutcome::Passed);
    assert!(!report.requirements_satisfied());
}

#[test]
fn test_unverified_completion_cannot_be_overwritten_by_success() {
    let mut report = ContractReport::new();
    report.expect(ContractCheckId::ReadBasic);
    report.record(
        ContractCheckId::ReadBasic,
        None,
        ContractCheckOutcome::Unverified {
            reason: "independent read unavailable".to_owned(),
        },
    );
    report.record(ContractCheckId::ReadBasic, None, ContractCheckOutcome::Passed);
    assert!(!report.requirements_satisfied());
}

#[test]
fn test_core_check_cannot_downgrade_itself_to_optional() {
    let mut report = ContractReport::new();
    report.expect(ContractCheckId::WriteBasic);
    report.record(
        ContractCheckId::WriteBasic,
        None,
        ContractCheckOutcome::SkippedOptional {
            reason: "fixture elected to skip writing".to_owned(),
        },
    );
    assert!(!report.requirements_satisfied());
}

#[test]
fn test_strict_report_diagnostics_cover_missing_reason_and_registration_cases() {
    let empty = ContractReport::new();
    assert!(std::panic::catch_unwind(|| empty.assert_satisfied()).is_err());

    let mut missing = ContractReport::new();
    missing.expect(ContractCheckId::ReadBasic);
    assert!(std::panic::catch_unwind(|| missing.assert_satisfied()).is_err());

    let mut optional = ContractReport::new();
    let optional_id = ContractCheckId::AsyncCopyCancelReader;
    optional.expect(optional_id);
    optional.record(
        optional_id,
        None,
        ContractCheckOutcome::SkippedOptional {
            reason: "optional path skipped".to_owned(),
        },
    );
    assert!(std::panic::catch_unwind(|| optional.assert_satisfied_with(&[optional_id])).is_err());

    let mut blank = ContractReport::new();
    blank.expect(ContractCheckId::ReadBasic);
    blank.record(
        ContractCheckId::ReadBasic,
        None,
        ContractCheckOutcome::NotApplicable {
            reason: " \t ".to_owned(),
        },
    );
    assert!(std::panic::catch_unwind(|| blank.assert_satisfied()).is_err());

    let mut outside = ContractReport::new();
    outside.expect(ContractCheckId::ReadBasic);
    outside.record(ContractCheckId::ReadBasic, None, ContractCheckOutcome::Passed);
    assert!(
        std::panic::catch_unwind(|| {
            outside.assert_satisfied_with(&[ContractCheckId::WriteBasic]);
        })
        .is_err()
    );

    let mut duplicate = ContractReport::new();
    duplicate.register(ContractCheckId::ReadBasic, None);
    duplicate.register(ContractCheckId::ReadBasic, None);
    assert!(!duplicate.requirements_satisfied());
}
