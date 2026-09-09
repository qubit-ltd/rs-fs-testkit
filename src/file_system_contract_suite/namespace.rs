// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Completeness checks using independent configured-namespace snapshots.

use qubit_fs::directory::ListFilter;
use qubit_fs::directory::ListOptions;
use qubit_fs::directory::ListScope;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::path::PathSemantics;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FileSystemContractSuite;
use crate::FixtureSupport;
use crate::internal::verify_condition;

impl FileSystemContractSuite<'_> {
    /// Checks a flat scope without deriving expectations from the tested list.
    pub(super) fn check_flat_scope_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        self.context.begin(id.as_str());
        let capability = FileSystemCapability::List;
        let hierarchical = self.context.properties().info().path_semantics() == PathSemantics::Hierarchical;
        let relative = self.context.relative_name("flat-scope");
        let scope = if id == ContractCheckId::ListNamespace {
            ListScope::Namespace
        } else {
            ListScope::Path(
                self.fixture
                    .path(&relative)
                    .map_err(|error| ContractFailure::with_source("raw prefix preparation failed", error).at(id))?,
            )
        };
        let options = ListOptions::object_keys().with_filter(Some(ListFilter::LiteralPrefix(String::new())));
        if hierarchical || !self.capable(capability) {
            let error = match self.fixture.file_system().list(&scope, options) {
                Err(error) => error,
                Ok(_) => return Err(ContractFailure::message_only("unsupported flat listing succeeded").at(id)),
            };
            let expected_kind = if hierarchical {
                FsErrorKind::InvalidOptions
            } else {
                FsErrorKind::UnsupportedCapability
            };
            let required = if hierarchical { None } else { Some(capability) };
            verify_condition(
                error.kind() == expected_kind
                    && error.operation() == FsOperation::List
                    && error.path() == scope.path()
                    && error.required_capability() == required
                    && error.provider() == Some(self.context.properties().info().provider_id()),
                id,
                "flat listing rejection context mismatch",
            )?;
            self.context
                .record_check(id, Some(capability), ContractCheckOutcome::RejectedAsExpected);
            return Ok(());
        }
        let names = if id == ContractCheckId::ListNamespace {
            vec![relative.clone(), format!("{relative}/nested/key")]
        } else {
            vec![
                relative.clone(),
                format!("{relative}/a"),
                format!("{relative}ish"),
                format!("unrelated-{relative}"),
            ]
        };
        let mut expected = Vec::new();
        for (index, name) in names.iter().enumerate() {
            let prepared = self
                .fixture
                .seed_file(name, b"flat entry")
                .map_err(|error| ContractFailure::with_source("flat listing seed failed", error).at(id))?;
            let FixtureSupport::Supported(path) = prepared else {
                self.context.record_check(
                    id,
                    Some(capability),
                    ContractCheckOutcome::Unverified {
                        reason: "flat listing needs independent file preparation".to_owned(),
                    },
                );
                return Ok(());
            };
            self.context.record_created(path.clone());
            if index < 3 {
                expected.push(path);
            }
        }
        if id == ContractCheckId::ListNamespace {
            let snapshot = self
                .fixture
                .snapshot_namespace_paths()
                .map_err(|error| ContractFailure::with_source("namespace snapshot failed", error).at(id))?;
            let FixtureSupport::Supported(paths) = snapshot else {
                self.context.record_check(
                    id,
                    Some(capability),
                    ContractCheckOutcome::Unverified {
                        reason: "namespace listing needs an independent complete snapshot".to_owned(),
                    },
                );
                return Ok(());
            };
            verify_condition(
                expected.iter().all(|path| paths.contains(path)),
                id,
                "namespace snapshot omitted seeded paths",
            )?;
            expected = paths;
        }
        let mut stream = self
            .fixture
            .file_system()
            .list(&scope, options.with_max_entries(Some(expected.len().saturating_add(1))))
            .map_err(|error| ContractFailure::with_source("flat listing open failed", error).at(id))?;
        let mut actual = Vec::new();
        while let Some(entry) = stream
            .next_entry()
            .map_err(|error| ContractFailure::with_source("flat listing stream failed", error).at(id))?
        {
            actual.push(entry.path);
            verify_condition(
                actual.len() <= expected.len(),
                id,
                "flat listing returned duplicate or unexpected entries",
            )?;
        }
        actual.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        expected.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        verify_condition(
            actual == expected,
            id,
            "flat listing omitted or duplicated namespace entries",
        )?;
        self.context
            .record_check(id, Some(capability), ContractCheckOutcome::Passed);
        Ok(())
    }
}
