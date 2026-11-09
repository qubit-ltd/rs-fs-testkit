// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Tests for property snapshots and bounded path admission planning.

use qubit_fs::metadata::FileSystemCapabilities;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::FileSystemId;
use qubit_fs::metadata::FileSystemInfo;
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs::metadata::FileSystemProperties;
use qubit_fs::metadata::SymlinkPolicy;
use qubit_fs::path::Path;
use qubit_fs::path::PathConstraints;
use qubit_fs::path::PathForm;
use qubit_fs::path::PathSemantics;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::internal::property_expectations::admission_path;
use crate::internal::property_expectations::check_snapshot;

fn properties(
    limits: FileSystemLimits,
    semantics: PathSemantics,
    form: PathForm,
    symlink_policy: SymlinkPolicy,
) -> FileSystemProperties {
    FileSystemProperties::new(
        FileSystemInfo::new(
            FileSystemId::new("property-test").expect("valid test filesystem id"),
            "property-test-provider",
            semantics,
        ),
        FileSystemCapabilities::new(),
        limits,
        match form {
            PathForm::Absolute => PathConstraints::absolute(),
            PathForm::Relative => PathConstraints::relative(),
            PathForm::Either => PathConstraints::either(),
        },
        symlink_policy,
    )
    .expect("test properties must be valid")
}

fn is_skipped(outcome: &Result<Result<Path, ContractCheckOutcome>, crate::ContractFailure>) -> bool {
    matches!(outcome, Ok(Err(ContractCheckOutcome::SkippedOptional { .. })))
}

#[test]
fn test_snapshot_groups_pass_and_report_mismatches() {
    let baseline = properties(
        FileSystemLimits::unknown(),
        PathSemantics::Hierarchical,
        PathForm::Absolute,
        SymlinkPolicy::Reject,
    );
    for id in [
        ContractCheckId::PropertiesSnapshot,
        ContractCheckId::PropertiesCapabilityDependencies,
        ContractCheckId::PropertiesLimits,
        ContractCheckId::PropertiesSymlinkPolicy,
    ] {
        assert!(matches!(
            check_snapshot(id, &baseline, &baseline),
            Ok(ContractCheckOutcome::Passed)
        ));
    }
    let changed_limits = properties(
        FileSystemLimits::unknown().with_max_write_bytes(FileSystemLimit::Maximum(4)),
        PathSemantics::Hierarchical,
        PathForm::Either,
        SymlinkPolicy::Reject,
    );
    let limits_failure = check_snapshot(
        ContractCheckId::PropertiesLimits,
        &baseline,
        &changed_limits,
    )
        .expect_err("a changed limits snapshot must fail");
    assert_eq!(limits_failure.check(), Some(ContractCheckId::PropertiesLimits));
    assert!(limits_failure.message().ends_with("limits snapshot changed"));
    let changed_policy = properties(
        FileSystemLimits::unknown(),
        PathSemantics::Hierarchical,
        PathForm::Absolute,
        SymlinkPolicy::FollowWithinFileSystem,
    );
    let policy_failure = check_snapshot(
        ContractCheckId::PropertiesSymlinkPolicy,
        &baseline,
        &changed_policy,
    )
        .expect_err("a changed symlink policy must fail");
    assert_eq!(policy_failure.check(), Some(ContractCheckId::PropertiesSymlinkPolicy));
    assert!(policy_failure
        .message()
        .ends_with("symlink policy snapshot changed"));
    let wrong_driver = check_snapshot(ContractCheckId::ReadBasic, &baseline, &baseline)
        .expect_err("a non-property check must be rejected by the property driver");
    assert_eq!(wrong_driver.check(), Some(ContractCheckId::ReadBasic));
    assert!(wrong_driver
        .message()
        .ends_with("selected check requires a different property driver"));
}

#[test]
fn test_list_page_boundary_and_non_finite_limit_are_handled() {
    let finite = properties(
        FileSystemLimits::unknown().with_max_list_page_entries(FileSystemLimit::Maximum(4)),
        PathSemantics::Hierarchical,
        PathForm::Absolute,
        SymlinkPolicy::Reject,
    );
    assert!(matches!(
        check_snapshot(ContractCheckId::PropertiesLimitListPage, &finite, &finite),
        Ok(ContractCheckOutcome::Passed)
    ));
    let unknown = properties(
        FileSystemLimits::unknown(),
        PathSemantics::Hierarchical,
        PathForm::Absolute,
        SymlinkPolicy::Reject,
    );
    assert!(matches!(
        check_snapshot(ContractCheckId::PropertiesLimitListPage, &unknown, &unknown),
        Ok(ContractCheckOutcome::SkippedOptional { .. })
    ));
}

#[test]
fn test_admission_paths_cover_not_applicable_and_masked_boundaries() {
    let literal = properties(
        FileSystemLimits::unknown().with_max_component_text_bytes(FileSystemLimit::Maximum(3)),
        PathSemantics::ObjectKey,
        PathForm::Either,
        SymlinkPolicy::Reject,
    );
    assert!(matches!(
        admission_path(ContractCheckId::PropertiesLimitComponentAdmission, &literal),
        Ok(Err(ContractCheckOutcome::NotApplicable { .. }))
    ));
    let no_limit = properties(
        FileSystemLimits::unknown(),
        PathSemantics::Hierarchical,
        PathForm::Absolute,
        SymlinkPolicy::Reject,
    );
    assert!(is_skipped(&admission_path(
        ContractCheckId::PropertiesLimitPathAdmission,
        &no_limit
    )));
    let masked_component = properties(
        FileSystemLimits::unknown()
            .with_max_component_text_bytes(FileSystemLimit::Maximum(4))
            .with_max_path_text_bytes(FileSystemLimit::Maximum(3)),
        PathSemantics::Hierarchical,
        PathForm::Absolute,
        SymlinkPolicy::Reject,
    );
    assert!(is_skipped(&admission_path(
        ContractCheckId::PropertiesLimitComponentAdmission,
        &masked_component
    )));
    let masked_path = properties(
        FileSystemLimits::unknown()
            .with_max_path_text_bytes(FileSystemLimit::Maximum(4))
            .with_max_component_text_bytes(FileSystemLimit::Maximum(2)),
        PathSemantics::Hierarchical,
        PathForm::Absolute,
        SymlinkPolicy::Reject,
    );
    assert!(is_skipped(&admission_path(
        ContractCheckId::PropertiesLimitPathAdmission,
        &masked_path
    )));
}

#[test]
fn test_admission_path_reports_absolute_budget_and_path_form_masking() {
    let absolute_budget = properties(
        FileSystemLimits::unknown().with_max_component_text_bytes(FileSystemLimit::Maximum(65_535)),
        PathSemantics::Hierarchical,
        PathForm::Absolute,
        SymlinkPolicy::Reject,
    );
    assert!(is_skipped(&admission_path(
        ContractCheckId::PropertiesLimitComponentAdmission,
        &absolute_budget
    )));
}

#[test]
fn test_admission_path_builds_bounded_absolute_relative_and_component_paths() {
    let absolute = properties(
        FileSystemLimits::unknown().with_max_path_text_bytes(FileSystemLimit::Maximum(8)),
        PathSemantics::Hierarchical,
        PathForm::Absolute,
        SymlinkPolicy::Reject,
    );
    let Ok(Ok(absolute_path)) = admission_path(ContractCheckId::PropertiesLimitPathAdmission, &absolute) else {
        panic!("absolute path boundary should be isolated");
    };
    assert_eq!(absolute_path.as_str().len(), 9);

    let relative = properties(
        FileSystemLimits::unknown().with_max_path_text_bytes(FileSystemLimit::Maximum(8)),
        PathSemantics::Hierarchical,
        PathForm::Relative,
        SymlinkPolicy::Reject,
    );
    let Ok(Ok(relative_path)) = admission_path(ContractCheckId::PropertiesLimitPathAdmission, &relative) else {
        panic!("relative path boundary should be isolated");
    };
    assert_eq!(relative_path.as_str().len(), 9);

    let component = properties(
        FileSystemLimits::unknown().with_max_component_text_bytes(FileSystemLimit::Maximum(4)),
        PathSemantics::Hierarchical,
        PathForm::Either,
        SymlinkPolicy::Reject,
    );
    let Ok(Ok(component_path)) = admission_path(ContractCheckId::PropertiesLimitComponentAdmission, &component) else {
        panic!("component boundary should be isolated");
    };
    assert_eq!(component_path.as_str(), "xxxxx");
}

#[test]
fn test_snapshot_rejects_changed_identity_and_capability_sets() {
    let baseline = properties(
        FileSystemLimits::unknown(),
        PathSemantics::Hierarchical,
        PathForm::Absolute,
        SymlinkPolicy::Reject,
    );
    let changed_identity = FileSystemProperties::new(
        FileSystemInfo::new(
            FileSystemId::new("different-test").expect("valid test filesystem id"),
            "property-test-provider",
            PathSemantics::Hierarchical,
        ),
        FileSystemCapabilities::new(),
        FileSystemLimits::unknown(),
        PathConstraints::absolute(),
        SymlinkPolicy::Reject,
    )
    .expect("test properties must be valid");
    let identity_failure = check_snapshot(
        ContractCheckId::PropertiesSnapshot,
        &baseline,
        &changed_identity,
    )
        .expect_err("a changed filesystem identity must fail");
    assert!(identity_failure
        .message()
        .ends_with("filesystem information snapshot changed"));

    let changed_provider = FileSystemProperties::new(
        FileSystemInfo::new(
            FileSystemId::new("property-test").expect("valid test filesystem id"),
            "different-provider",
            PathSemantics::Hierarchical,
        ),
        FileSystemCapabilities::new(),
        FileSystemLimits::unknown(),
        PathConstraints::absolute(),
        SymlinkPolicy::Reject,
    )
    .expect("test properties must be valid");
    let provider_failure = check_snapshot(
        ContractCheckId::PropertiesSnapshot,
        &baseline,
        &changed_provider,
    )
        .expect_err("a changed provider identity must fail");
    assert!(provider_failure
        .message()
        .ends_with("filesystem information snapshot changed"));

    let changed_capabilities = FileSystemProperties::new(
        baseline.info().clone(),
        FileSystemCapabilities::new().with_guaranteed(FileSystemCapability::Read),
        FileSystemLimits::unknown(),
        PathConstraints::absolute(),
        SymlinkPolicy::Reject,
    )
    .expect("test properties must be valid");
    let capability_failure = check_snapshot(
        ContractCheckId::PropertiesSnapshot,
        &baseline,
        &changed_capabilities,
    )
        .expect_err("a changed capability set must fail");
    assert!(capability_failure
        .message()
        .ends_with("capability snapshot changed"));
}
