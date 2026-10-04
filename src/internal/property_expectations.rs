// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared, I/O-free property expectations and bounded admission requests.

use qubit_fs as qfs;
use qubit_fs::metadata::FileSystemProperties;
use qubit_fs::path::Path;
use qubit_fs::path::PathSemantics;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::internal::limit_probe_plan::MAX_PROBE_BYTES;
use crate::internal::limit_probe_plan::MAX_PROBE_ENTRIES;
use crate::internal::limit_probe_plan::finite_probe;
use crate::internal::verify_condition;

/// Verifies one immutable property group without preparing unrelated resources.
pub(crate) fn check_snapshot(
    id: ContractCheckId,
    expected: &FileSystemProperties,
    actual: &FileSystemProperties,
) -> Result<ContractCheckOutcome, ContractFailure> {
    match id {
        ContractCheckId::PropertiesSnapshot => {
            verify_condition(!expected.info().id().as_str().is_empty(), id, "filesystem id is empty")?;
            verify_condition(!expected.info().provider_id().is_empty(), id, "provider id is empty")?;
            verify_condition(
                expected.info() == actual.info(),
                id,
                "filesystem information snapshot changed",
            )?;
            verify_condition(
                expected.capabilities() == actual.capabilities(),
                id,
                "capability snapshot changed",
            )?;
        }
        ContractCheckId::PropertiesCapabilityDependencies => {
            verify_condition(
                expected.capabilities().missing_dependency().is_none(),
                id,
                "capability dependencies are inconsistent",
            )?;
        }
        ContractCheckId::PropertiesLimits => {
            verify_condition(expected.limits() == actual.limits(), id, "limits snapshot changed")?;
        }
        ContractCheckId::PropertiesSymlinkPolicy => {
            verify_condition(
                expected.symlink_policy() == actual.symlink_policy(),
                id,
                "symlink policy snapshot changed",
            )?;
        }
        ContractCheckId::PropertiesLimitListPage => {
            let Some((maximum, over)) = finite_probe(expected.limits().max_list_page_entries(), MAX_PROBE_ENTRIES)
            else {
                return Ok(ContractCheckOutcome::SkippedOptional {
                    reason: "list page limit has no finite successor within the probe budget".to_owned(),
                });
            };
            let requested = probe_length(over, id, "list page boundary is not representable")?;
            let effective = expected.limits().clamp_list_page_size(Some(requested));
            verify_condition(
                effective.is_some_and(|value| value as u64 <= maximum),
                id,
                "list page clamp omitted or exceeded the declared maximum",
            )?;
        }
        _ => {
            return Err(ContractFailure::message_only("selected check requires a different property driver").at(id));
        }
    }
    Ok(ContractCheckOutcome::Passed)
}

/// Plans an oversized path or returns a justified outcome when it cannot be
/// isolated.
pub(crate) fn admission_path(
    id: ContractCheckId,
    properties: &FileSystemProperties,
) -> Result<Result<Path, ContractCheckOutcome>, ContractFailure> {
    let semantics = properties.info().path_semantics();
    let limits = properties.limits();
    let component = id == ContractCheckId::PropertiesLimitComponentAdmission;
    if component && semantics != PathSemantics::Hierarchical {
        return Ok(Err(ContractCheckOutcome::NotApplicable {
            reason: "component limits do not apply to literal paths".to_owned(),
        }));
    }
    let limit = if component {
        limits.max_component_text_bytes()
    } else {
        limits.max_path_text_bytes()
    };
    let Some((_, over)) = finite_probe(limit, MAX_PROBE_BYTES) else {
        return Ok(Err(ContractCheckOutcome::SkippedOptional {
            reason: "path boundary has no finite successor within the probe budget".to_owned(),
        }));
    };
    let absolute = properties.path_constraints().form() == qfs::path::PathForm::Absolute;
    let prefix_bytes = u64::from(absolute);
    let component_bytes = if component { over } else { over - prefix_bytes };
    let text_bytes = component_bytes + prefix_bytes;
    if text_bytes > MAX_PROBE_BYTES {
        return Ok(Err(ContractCheckOutcome::SkippedOptional {
            reason: "absolute path boundary exceeds the total probe budget".to_owned(),
        }));
    }
    // Do not attribute a rejection from a different limit to the selected one.
    let masked = if component {
        limits
            .max_path_text_bytes()
            .maximum()
            .is_some_and(|maximum| text_bytes > maximum)
    } else {
        semantics == PathSemantics::Hierarchical
            && limits
                .max_component_text_bytes()
                .maximum()
                .is_some_and(|maximum| component_bytes > maximum)
    };
    if masked {
        return Ok(Err(ContractCheckOutcome::SkippedOptional {
            reason: "another path limit would mask this boundary's rejection".to_owned(),
        }));
    }
    let length = probe_length(component_bytes, id, "path boundary is not representable")?;
    let text = format!("{}{}", if absolute { "/" } else { "" }, "x".repeat(length));
    let path = Path::parse_with_semantics(&text, semantics)
        .map_err(|error| ContractFailure::with_source("path boundary could not be parsed", error).at(id))?;
    Ok(Ok(path))
}

fn probe_length(value: u64, id: ContractCheckId, message: &'static str) -> Result<usize, ContractFailure> {
    usize::try_from(value).map_err(|error| ContractFailure::with_source(message, error).at(id))
}

#[cfg(test)]
mod tests {
    use qubit_fs::metadata::FileSystemCapabilities;
    use qubit_fs::metadata::FileSystemCapability;
    use qubit_fs::metadata::FileSystemId;
    use qubit_fs::metadata::FileSystemInfo;
    use qubit_fs::metadata::FileSystemLimit;
    use qubit_fs::metadata::FileSystemLimits;
    use qubit_fs::metadata::SymlinkPolicy;
    use qubit_fs::path::Path;
    use qubit_fs::path::PathConstraints;
    use qubit_fs::path::PathForm;
    use qubit_fs::path::PathSemantics;

    use super::FileSystemProperties;
    use super::admission_path;
    use super::check_snapshot;
    use crate::ContractCheckId;
    use crate::ContractCheckOutcome;

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
    fn snapshot_groups_pass_and_report_mismatches() {
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
        assert!(check_snapshot(ContractCheckId::PropertiesLimits, &baseline, &changed_limits).is_err());
        let changed_policy = properties(
            FileSystemLimits::unknown(),
            PathSemantics::Hierarchical,
            PathForm::Absolute,
            SymlinkPolicy::FollowWithinFileSystem,
        );
        assert!(check_snapshot(ContractCheckId::PropertiesSymlinkPolicy, &baseline, &changed_policy).is_err());
        assert!(check_snapshot(ContractCheckId::ReadBasic, &baseline, &baseline).is_err());
    }

    #[test]
    fn list_page_boundary_and_non_finite_limit_are_handled() {
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
    fn admission_paths_cover_not_applicable_and_masked_boundaries() {
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
    fn admission_path_reports_absolute_budget_and_path_form_masking() {
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
    fn probe_length_accepts_a_bounded_boundary() {
        let length = super::probe_length(
            4,
            ContractCheckId::PropertiesLimitPathAdmission,
            "path boundary is not representable",
        )
        .expect("bounded path probe fits usize");
        assert_eq!(length, 4);
    }

    #[test]
    fn admission_path_builds_bounded_absolute_relative_and_component_paths() {
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
        let Ok(Ok(component_path)) = admission_path(ContractCheckId::PropertiesLimitComponentAdmission, &component)
        else {
            panic!("component boundary should be isolated");
        };
        assert_eq!(component_path.as_str(), "xxxxx");
    }

    #[test]
    fn snapshot_rejects_changed_identity_and_capability_sets() {
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
        assert!(check_snapshot(ContractCheckId::PropertiesSnapshot, &baseline, &changed_identity).is_err());

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
        assert!(check_snapshot(ContractCheckId::PropertiesSnapshot, &baseline, &changed_provider).is_err());

        let changed_capabilities = FileSystemProperties::new(
            baseline.info().clone(),
            FileSystemCapabilities::new().with_guaranteed(FileSystemCapability::Read),
            FileSystemLimits::unknown(),
            PathConstraints::absolute(),
            SymlinkPolicy::Reject,
        )
        .expect("test properties must be valid");
        assert!(check_snapshot(ContractCheckId::PropertiesSnapshot, &baseline, &changed_capabilities).is_err());
    }
}
