// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Evidence for owning whole-file write operation lifecycles.

use qubit_fs as qfs;
use qubit_fs::error::FsErrorKind;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::write::AsyncWriteAllOperationState;
use qubit_fs::write::WriteFailureState;
use qubit_fs::write::WriteOptions;

use crate::AsyncFileSystemContractSuite;
use crate::ContractAsyncWriteFailure;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl AsyncFileSystemContractSuite<'_> {
    /// Exercises publication, unpolled drop, and repeated execution.
    pub(super) async fn check_owning_write(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        let relative = self
            .context
            .relative_name(if id == ContractCheckId::WriteRepeatedExecute {
                "owning-repeat"
            } else {
                "owning-write"
            });
        let bytes = super::write::bounded_payload(
            self.context.properties().limits().max_write_bytes(),
            b"owning write",
            b'o',
        );
        if !self.capable(FileSystemCapability::Write) {
            let path = self
                .fixture
                .path(&relative)
                .map_err(|error| ContractFailure::with_source("owning write path preparation failed", error).at(id))?;
            let failure = match crate::internal::execute_write::execute_write(
                self.fixture.file_system(),
                path,
                bytes.clone(),
                WriteOptions::default(),
            )
            .await
            {
                Err(failure) => failure,
                Ok(_) => return Err(ContractFailure::message_only("unsupported owning write succeeded").at(id)),
            };
            if failure.error().kind() != FsErrorKind::UnsupportedCapability {
                return Err(ContractFailure::with_owned_source("owning write rejection kind differs", failure).at(id));
            }
            self.context.record_check(
                id,
                Some(FileSystemCapability::Write),
                ContractCheckOutcome::RejectedAsExpected,
            );
            return Ok(());
        }
        let scenario = crate::internal::check_catalog::specification(id)
            .write_scenario
            .ok_or_else(|| ContractFailure::message_only("owning write lacks a preparation scenario").at(id))?;
        let prepared = self
            .fixture
            .prepare_write(scenario, &relative, &bytes)
            .await
            .map_err(|error| ContractFailure::with_source("owning write scenario preparation failed", error).at(id))?;
        let case = match prepared {
            crate::FixturePreparation::Ready(case) => case,
            crate::FixturePreparation::Unavailable { reason } | crate::FixturePreparation::NotApplicable { reason } => {
                self.context.record_check(
                    id,
                    Some(FileSystemCapability::Write),
                    ContractCheckOutcome::Unverified { reason },
                );
                return Ok(());
            }
        };
        verify_condition(
            case.bytes() == bytes
                && matches!(
                    case.options().disposition(),
                    qfs::write::WriteDisposition::CreateNew | qfs::write::WriteDisposition::CreateOrReplace
                ),
            id,
            "owning write fixture changed creation semantics",
        )?;
        let path = case.path().clone();
        self.context.record_created(path.clone());
        let mut operation = self
            .fixture
            .file_system()
            .begin_write_all(path.clone(), bytes.clone(), case.options().clone())
            .map_err(|error| {
                ContractFailure::with_owned_source(
                    "owning write request preparation failed",
                    ContractAsyncWriteFailure::new(error, None),
                )
                .at(id)
            })?;
        drop(operation.execute());
        verify_condition(
            operation.state() == AsyncWriteAllOperationState::Ready,
            id,
            "unpolled future changed state",
        )?;
        verify_condition(
            !operation.has_recovery_writer(),
            id,
            "unpolled future acquired a writer",
        )?;
        if let Err(error) = operation.execute().await {
            return Err(ContractFailure::with_owned_source(
                "owning write execution failed",
                ContractAsyncWriteFailure::new(error, Some(operation)),
            )
            .at(id));
        }
        verify_condition(
            operation.state() == AsyncWriteAllOperationState::Completed,
            id,
            "success did not complete the operation",
        )?;
        verify_condition(
            operation.written_bytes() == bytes.len() as u64,
            id,
            "accepted byte count mismatch",
        )?;
        let observed = self
            .fixture
            .read_file(&path)
            .await
            .map_err(|error| ContractFailure::with_source("owning write observation failed", error).at(id))?;
        verify_condition(
            matches!(observed, FixtureSupport::Supported(actual) if actual == bytes),
            id,
            "independent publication evidence is missing or differs",
        )?;
        if id == ContractCheckId::WriteOwningOperation {
            self.context
                .record_check(id, Some(FileSystemCapability::Write), ContractCheckOutcome::Passed);
            return Ok(());
        }
        let repeated = match operation.execute().await {
            Err(failure) => failure,
            Ok(_) => return Err(ContractFailure::message_only("owning write executed twice").at(id)),
        };
        if repeated.error().kind() != FsErrorKind::InvalidState
            || repeated.state() != WriteFailureState::Published
            || repeated.written_bytes() != bytes.len() as u64
        {
            return Err(ContractFailure::with_owned_source(
                "repeated owning write lost its completed facts",
                ContractAsyncWriteFailure::new(repeated, Some(operation)),
            )
            .at(id));
        }
        verify_condition(
            operation.state() == AsyncWriteAllOperationState::Completed,
            id,
            "repeat changed completed state",
        )?;
        let observed =
            self.fixture.read_file(&path).await.map_err(|error| {
                ContractFailure::with_source("repeated owning write observation failed", error).at(id)
            })?;
        verify_condition(
            matches!(observed, FixtureSupport::Supported(actual) if actual == bytes),
            id,
            "repeat changed published bytes or evidence is missing",
        )?;
        self.context
            .record_check(id, Some(FileSystemCapability::Write), ContractCheckOutcome::Passed);
        Ok(())
    }
}
