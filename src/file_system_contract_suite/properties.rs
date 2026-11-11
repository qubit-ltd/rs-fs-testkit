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
    ///
    /// # Parameters
    ///
    /// - `id`: Registered property check to evaluate against the snapshot.
    pub(super) fn check_properties_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        self.context.begin_check(id);
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
///
/// # Parameters
///
/// - `fixture`: Independent fixture hooks used to build a representative path.
/// - `expected`: Properties snapshot captured when the suite began.
/// - `actual`: Properties returned by the provider during this check.
/// - `id`: Registered path-constraint check receiving any failure.
///
/// # Returns
///
/// `Ok(())` when the fixture path is valid and constraints match the snapshot.
///
/// # Errors
///
/// Returns a contract failure if the path is invalid or the provider changed
/// its advertised constraints.
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
///
/// # Parameters
///
/// - `fixture`: Independent fixture used to construct and submit the path.
/// - `expected`: Captured properties used to derive the boundary request.
/// - `id`: Registered path-admission check to evaluate.
///
/// # Returns
///
/// The expected rejection outcome, or another outcome when no safe probe is
/// available under the declared limit. The latter is skipped when the limit is
/// not finite within the probe budget, inapplicable to the path model, or
/// masked by another path limit.
///
/// # Errors
///
/// Returns a contract failure if the path is admitted or rejection differs
/// from the advertised limit.
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
