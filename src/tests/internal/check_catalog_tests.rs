// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Tests for ownership and applicability in the internal check catalog.

use std::collections::HashSet;

use crate::ContractCheckId;
use crate::FileSystemContract;
use crate::internal::check_catalog::for_contract;
use crate::internal::check_catalog::required_copy_scenario;
use crate::internal::check_catalog::specification;

#[test]
fn test_each_executable_check_has_one_owner() {
    let mut seen = HashSet::new();
    for phase in FileSystemContract::ALL {
        for check in for_contract(phase, true) {
            assert!(seen.insert(check.id), "duplicate owner for {}", check.id);
            assert_eq!(
                check.read_scenario.is_some(),
                phase == FileSystemContract::Read,
                "read preparation metadata must match execution ownership for {}",
                check.id
            );
        }
    }
    for id in ContractCheckId::ALL {
        assert!(seen.contains(id), "missing catalog entry for {id}");
    }
}

#[test]
fn test_prerequisites_are_cataloged_in_the_same_phase() {
    for id in ContractCheckId::ALL {
        let spec = specification(*id);
        assert!(
            !spec.prerequisites().contains(id),
            "check cannot depend on itself: {id}"
        );
        for prerequisite in spec.prerequisites() {
            assert!(
                ContractCheckId::ALL.contains(prerequisite),
                "unknown prerequisite for {id}"
            );
            assert_eq!(
                specification(*prerequisite).contract,
                spec.contract,
                "prerequisite must be owned by the same phase: {id}"
            );
        }
    }
}

#[test]
fn test_required_copy_scenario_returns_and_rejects_catalog_entries() {
    assert!(matches!(
        required_copy_scenario(ContractCheckId::CopyBasic, "missing copy scenario"),
        Ok(crate::CopyScenario::Basic)
    ));
    let failure = required_copy_scenario(ContractCheckId::ReadBasic, "missing copy scenario")
        .expect_err("read checks have no copy scenario");
    assert_eq!(failure.check(), Some(ContractCheckId::ReadBasic));
    assert_eq!(failure.message(), "missing copy scenario");
}
