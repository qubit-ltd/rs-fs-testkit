// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Independent immutable property and bounded admission checks.

use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemProperties;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FileSystemContract;
use crate::FileSystemContractSuite;
use crate::FileSystemFixture;
use crate::internal::check_catalog;
use crate::internal::property_expectations;
use crate::internal::verify_condition;
use crate::internal::verify_fs_error;

impl FileSystemContractSuite<'_> {
    /// Executes the catalog's property entries in independent scopes.
    ///
    /// # Returns
    ///
    /// `Ok(())` after all catalog entries pass.
    ///
    /// # Errors
    ///
    /// Returns the first contract failure reported while checking an entry.
    pub(super) fn check_properties(&mut self) -> Result<(), ContractFailure> {
        for spec in check_catalog::for_contract(FileSystemContract::Properties, false) {
            self.check_properties_item(spec.id)?;
        }
        Ok(())
    }

    /// Records one selected property check without preparing sibling checks.
    pub(super) fn check_properties_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        self.context.begin(id.as_str());
        let expected = self.context.properties();
        let actual = self.fixture.file_system().properties();
        let outcome = match id {
            ContractCheckId::PropertiesPathConstraints => {
                check_path_constraints(self.fixture, expected, actual, id)?;
                ContractCheckOutcome::Passed
            }
            ContractCheckId::PropertiesLimitPathAdmission | ContractCheckId::PropertiesLimitComponentAdmission => {
                check_path_admission(self.fixture, expected, id)?
            }
            _ => property_expectations::check_snapshot(id, expected, actual)?,
        };
        let spec = check_catalog::specification(id);
        self.context.record_check(id, spec.capability, outcome);
        Ok(())
    }
}

/// Confirms fixture path validation and the provider's advertised path forms.
fn check_path_constraints(
    fixture: &dyn FileSystemFixture,
    expected: &FileSystemProperties,
    actual: &FileSystemProperties,
    id: ContractCheckId,
) -> Result<(), ContractFailure> {
    let path = fixture
        .path("contract-properties")
        .map_err(|error| ContractFailure::with_source("properties fixture path failed", error).at(id))?;
    expected
        .path_constraints()
        .validate(&path)
        .map_err(|error| ContractFailure::with_source("fixture path violates constraints", error).at(id))?;
    verify_condition(
        expected.path_constraints() == actual.path_constraints(),
        id,
        "path constraints snapshot changed",
    )
}

/// Exercises a bounded path request and verifies the provider's rejection.
fn check_path_admission(
    fixture: &dyn FileSystemFixture,
    expected: &FileSystemProperties,
    id: ContractCheckId,
) -> Result<ContractCheckOutcome, ContractFailure> {
    match property_expectations::admission_path(id, expected)? {
        Err(outcome) => Ok(outcome),
        Ok(path) => {
            let error = match fixture.file_system().stat(&path) {
                Err(error) => error,
                Ok(_) => {
                    return Err(ContractFailure::message_only("path limit admitted an oversized request").at(id));
                }
            };
            verify_fs_error(
                error,
                FsErrorKind::ResourceLimitExceeded,
                FsOperation::Stat,
                &path,
                expected.info().provider_id(),
                None,
                id,
            )?;
            Ok(ContractCheckOutcome::RejectedAsExpected)
        }
    }
}
