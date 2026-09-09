// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Independent hierarchy and raw object-key listing checks.

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
use crate::internal::verify_fs_error;

impl FileSystemContractSuite<'_> {
    /// Runs one isolated request for each listing requirement.
    pub(super) fn check_list(&mut self) -> Result<(), ContractFailure> {
        for id in [
            ContractCheckId::ListBasic,
            ContractCheckId::ListPrefix,
            ContractCheckId::ListPagination,
            ContractCheckId::ListLiteralPrefix,
            ContractCheckId::ListNamespace,
            ContractCheckId::ListRawRootPrefix,
        ] {
            self.check_list_item(id)?;
        }
        Ok(())
    }

    /// Fixes expected entries from fixture preparation before reading the
    /// stream.
    pub(super) fn check_list_item(&mut self, id: ContractCheckId) -> Result<(), ContractFailure> {
        if matches!(id, ContractCheckId::ListNamespace | ContractCheckId::ListRawRootPrefix) {
            return self.check_flat_scope_item(id);
        }
        if !matches!(
            id,
            ContractCheckId::ListBasic
                | ContractCheckId::ListPrefix
                | ContractCheckId::ListPagination
                | ContractCheckId::ListLiteralPrefix
        ) {
            return Err(ContractFailure::message_only("selected entry is not listing").at(id));
        }
        self.context.begin(id.as_str());
        let hierarchical = self.context.properties().info().path_semantics() == PathSemantics::Hierarchical;
        let relative = self.context.relative_name("list-root");
        let root = self
            .fixture
            .path(&if hierarchical {
                relative.clone()
            } else {
                format!("{relative}/")
            })
            .map_err(|error| ContractFailure::with_source("list root preparation failed", error).at(id))?;
        let mut options = if hierarchical {
            ListOptions::default()
        } else {
            ListOptions::object_keys()
        };
        if id == ContractCheckId::ListLiteralPrefix {
            options = ListOptions::object_keys().with_filter(Some(ListFilter::LiteralPrefix("literal[1]".to_owned())));
        } else if id == ContractCheckId::ListPrefix {
            options = ListOptions::default()
                .with_recursive(true)
                .with_filter(Some(ListFilter::Subtree("prefixed".to_owned())));
        }
        let incompatible = (hierarchical && id == ContractCheckId::ListLiteralPrefix)
            || (!hierarchical && id == ContractCheckId::ListPrefix);
        let capability = FileSystemCapability::List;
        if incompatible || !self.capable(capability) {
            let error = match self.fixture.file_system().list(&ListScope::Path(root.clone()), options) {
                Err(error) => error,
                Ok(_) => return Err(ContractFailure::message_only("unsupported list request succeeded").at(id)),
            };
            let kind = if incompatible {
                FsErrorKind::InvalidOptions
            } else {
                FsErrorKind::UnsupportedCapability
            };
            let required = if incompatible { None } else { Some(capability) };
            verify_fs_error(
                error,
                kind,
                FsOperation::List,
                &root,
                self.context.properties().info().provider_id(),
                required,
                id,
            )?;
            self.context
                .record_check(id, Some(capability), ContractCheckOutcome::RejectedAsExpected);
            return Ok(());
        }
        let mut expected = Vec::new();
        if id == ContractCheckId::ListPrefix {
            let prefix = format!("{relative}/prefixed");
            let prepared = self
                .fixture
                .seed_empty_directory(&prefix)
                .map_err(|error| ContractFailure::with_source("list subtree preparation failed", error).at(id))?;
            let FixtureSupport::Supported(path) = prepared else {
                self.context.record_check(
                    id,
                    Some(capability),
                    ContractCheckOutcome::Unverified {
                        reason: "subtree listing needs an independently prepared directory".to_owned(),
                    },
                );
                return Ok(());
            };
            self.context.record_created(path.clone());
            expected.push(path);
        }
        let names: &[&str] = match id {
            ContractCheckId::ListPrefix => &["prefixed/first", "prefixed/second", "decoy"],
            ContractCheckId::ListLiteralPrefix => &["literal[1]-match", "literal1-decoy"],
            ContractCheckId::ListPagination => &["first", "second", "third"],
            _ if !hierarchical => &["first", "nested/second"],
            _ => &["first", "second"],
        };
        for name in names {
            let prepared = self
                .fixture
                .seed_file(&format!("{relative}/{name}"), b"list entry")
                .map_err(|error| ContractFailure::with_source("list entry preparation failed", error).at(id))?;
            let FixtureSupport::Supported(path) = prepared else {
                self.context.record_check(
                    id,
                    Some(capability),
                    ContractCheckOutcome::Unverified {
                        reason: "listing needs independently prepared entries".to_owned(),
                    },
                );
                return Ok(());
            };
            self.context.record_created(path.clone());
            let selected = match id {
                ContractCheckId::ListPrefix => name.starts_with("prefixed/"),
                ContractCheckId::ListLiteralPrefix => name.starts_with("literal[1]"),
                _ => true,
            };
            if selected {
                expected.push(path);
            }
        }
        let metadata = matches!(id, ContractCheckId::ListPrefix | ContractCheckId::ListPagination);
        options = options
            .with_include_metadata(metadata)
            .with_max_entries(Some(expected.len() + 1));
        if id == ContractCheckId::ListPagination {
            options = options.with_page_size(Some(1));
        }
        let mut stream = self
            .fixture
            .file_system()
            .list(&ListScope::Path(root.clone()), options)
            .map_err(|error| ContractFailure::with_source("list request failed", error).at(id))?;
        let mut actual = Vec::new();
        while let Some(entry) = stream
            .next_entry()
            .map_err(|error| ContractFailure::with_source("list stream error", error).at(id))?
        {
            if metadata {
                verify_condition(
                    entry.metadata.is_some(),
                    id,
                    "list contract: requested entry metadata is missing",
                )?;
            }
            actual.push(entry.path);
            verify_condition(
                actual.len() <= expected.len(),
                id,
                "listing returned duplicate or unexpected entries",
            )?;
        }
        actual.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        expected.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        verify_condition(
            actual == expected,
            id,
            "list/basic: direct children mismatch or filtered entries mismatch",
        )?;
        self.context
            .record_check(id, Some(capability), ContractCheckOutcome::Passed);
        Ok(())
    }
}
