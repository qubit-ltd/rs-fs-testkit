// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
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
            let requested = usize::try_from(over).map_err(|error| {
                ContractFailure::with_source("list page boundary is not representable", error).at(id)
            })?;
            let effective = expected.limits().clamp_list_page_size(Some(requested));
            verify_condition(
                effective.is_some_and(|value| value as u64 <= maximum),
                id,
                "list page clamp omitted or exceeded the declared maximum",
            )?;
        }
        _ => return Err(ContractFailure::message_only("selected check requires a different property driver").at(id)),
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
    let length = usize::try_from(component_bytes)
        .map_err(|error| ContractFailure::with_source("path boundary is not representable", error).at(id))?;
    let text = format!("{}{}", if absolute { "/" } else { "" }, "x".repeat(length));
    let path = Path::parse_with_semantics(&text, semantics)
        .map_err(|error| ContractFailure::with_source("path boundary could not be parsed", error).at(id))?;
    if properties.path_constraints().validate(&path).is_err() {
        return Ok(Err(ContractCheckOutcome::SkippedOptional {
            reason: "path constraints would mask the limit boundary rejection".to_owned(),
        }));
    }
    Ok(Ok(path))
}
