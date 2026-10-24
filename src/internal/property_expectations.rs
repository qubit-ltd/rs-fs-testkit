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
///
/// # Parameters
///
/// - `id`: Identifies the property group whose contract must be checked.
/// - `expected`: The captured property snapshot used as the baseline.
/// - `actual`: The current property snapshot to compare with the baseline.
///
/// # Returns
///
/// Returns `Passed` when the selected snapshot contract holds.
///
/// # Errors
///
/// Returns a contract failure when a property is inconsistent or differs from
/// its expected snapshot, or when `id` belongs to another driver.
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
///
/// # Parameters
///
/// - `id`: Identifies the path limit to probe.
/// - `properties`: Supplies path semantics, form, and declared limits.
///
/// # Returns
///
/// Returns the isolated path, or a skipped/not-applicable outcome when the
/// selected boundary cannot be tested independently within the probe budget.
///
/// # Errors
///
/// Returns a contract failure when the boundary length cannot be represented
/// or the generated path cannot be parsed.
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

/// Converts a bounded probe length to the platform's index type.
///
/// The conversion reports a contract failure when the requested length cannot
/// be represented as a `usize` on the current platform.
///
/// # Parameters
///
/// - `value`: The number of bytes or entries requested by the probe.
/// - `id`: The check associated with this conversion.
/// - `message`: The failure message used when conversion is not possible.
///
/// # Returns
///
/// Returns the length as `usize` when it is representable.
///
/// # Errors
///
/// Returns a contract failure when `value` exceeds the platform's `usize`
/// range.
fn probe_length(value: u64, id: ContractCheckId, message: &'static str) -> Result<usize, ContractFailure> {
    usize::try_from(value).map_err(|error| ContractFailure::with_source(message, error).at(id))
}

#[cfg(test)]
mod tests {
    use crate::ContractCheckId;

    #[test]
    fn test_probe_length_accepts_a_bounded_boundary() {
        let length = super::probe_length(
            4,
            ContractCheckId::PropertiesLimitPathAdmission,
            "path boundary is not representable",
        )
        .expect("bounded path probe fits usize");
        assert_eq!(length, 4);
    }
}
