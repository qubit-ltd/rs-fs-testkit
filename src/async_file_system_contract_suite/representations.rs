// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Independent empty-directory and symbolic-link representation evidence.

use qubit_fs::metadata::FileKind;
use qubit_fs::metadata::FileSystemCapability;

use crate::AsyncFileSystemContractSuite;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl AsyncFileSystemContractSuite<'_> {
    /// Executes the empty-directory and symbolic-link checks independently.
    ///
    /// Each entry prepares its own fixture and may perform asynchronous
    /// filesystem I/O. Preparation and observation failures stop the sequence.
    ///
    /// # Returns
    ///
    /// `Ok(())` after both checks complete, including checks recorded as
    /// not applicable or unverified.
    ///
    /// # Errors
    ///
    /// Returns the first contract failure from either representation check.
    pub(super) async fn check_representations(&mut self) -> Result<(), ContractFailure> {
        for id in [
            ContractCheckId::RepresentationEmpty,
            ContractCheckId::RepresentationSymlink,
        ] {
            self.check_representation_item(id).await?;
        }
        Ok(())
    }

    /// Seeds and verifies the representation selected by `id` when its
    /// capability is advertised by the provider.
    ///
    /// Unsupported fixture preparation is recorded as unverified. Successful
    /// preparation is observed through asynchronous metadata I/O. A non-
    /// representation identifier or a failed preparation, observation, or
    /// verification returns a contract failure.
    ///
    /// # Parameters
    ///
    /// * `id` - Identifier selecting the empty-directory or symbolic-link
    ///   representation check.
    ///
    /// # Returns
    ///
    /// `Ok(())` when the check passes or is recorded as not applicable or
    /// unverified.
    ///
    /// # Errors
    ///
    /// Returns a contract failure if `id` is not a representation check or
    /// fixture preparation, metadata observation, or verification fails.
    pub(super) async fn check_representation_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        let (capability, name) = match id {
            ContractCheckId::RepresentationEmpty => (FileSystemCapability::EmptyDirectory, "empty-directory"),
            ContractCheckId::RepresentationSymlink => (FileSystemCapability::Symlink, "symlink"),
            _ => {
                return Err(ContractFailure::message_only("selected entry is not a representation check").at(id));
            }
        };
        self.context.begin(id.as_str());
        if !self.capable(capability) {
            self.context.record_check(
                id,
                Some(capability),
                ContractCheckOutcome::NotApplicable {
                    reason: "provider does not advertise this representation; no rejection was exercised".to_owned(),
                },
            );
            return Ok(());
        }
        let relative = self.context.relative_name(name);
        let prepared = if id == ContractCheckId::RepresentationEmpty {
            self.fixture.seed_empty_directory(&relative).await
        } else {
            self.fixture.seed_symlink(&relative).await
        }
        .map_err(|error| ContractFailure::with_source("representation preparation failed", error).at(id))?;
        let path = match prepared {
            FixtureSupport::Supported(path) => path,
            FixtureSupport::Unsupported => {
                self.context.record_check(
                    id,
                    Some(capability),
                    ContractCheckOutcome::Unverified {
                        reason: "advertised representation requires independent fixture preparation".to_owned(),
                    },
                );
                return Ok(());
            }
        };
        self.context.record_created(path.clone());
        let metadata = self.fixture.file_system().stat(&path).await.map_err(|error| {
            ContractFailure::with_source("representation metadata observation failed", error).at(id)
        })?;
        if id == ContractCheckId::RepresentationEmpty {
            verify_condition(
                metadata.is_directory_like(),
                id,
                "representation contract: empty directory is not directory-like",
            )?;
        } else {
            verify_condition(
                metadata.kind() == &FileKind::Symlink,
                id,
                "representation contract: seeded link kind mismatch",
            )?;
        }
        self.context
            .record_check(id, Some(capability), ContractCheckOutcome::Passed);
        Ok(())
    }
}
