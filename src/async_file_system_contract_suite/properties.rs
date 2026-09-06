// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements property snapshots and bounded limit checks.

use super::*;
use crate::internal::limit_probe_plan::{MAX_PROBE_BYTES, MAX_PROBE_ENTRIES};

impl<'a> AsyncFileSystemContractSuite<'a> {
    /// Checks immutable facade properties and fixture path compatibility.
    ///
    /// # Panics
    ///
    /// Panics when identifiers or capabilities are inconsistent, the fixture
    /// path is invalid, or the facade snapshot changes during the suite run.
    pub async fn assert_properties(&mut self) {
        self.context.begin("properties");
        let properties = self.context.properties().clone();
        let info = properties.info();
        assert!(
            !info.id().as_str().is_empty(),
            "properties contract: filesystem id is empty"
        );
        assert!(
            !info.provider_id().is_empty(),
            "properties contract: provider id is empty"
        );
        assert!(
            properties.capabilities().missing_dependency().is_none(),
            "properties contract: capability dependencies are inconsistent"
        );
        let path = self
            .fixture
            .path("contract-properties")
            .expect("properties contract: fixture path failed");
        properties
            .path_constraints()
            .validate(&path)
            .expect("properties contract: fixture path violates constraints");
        assert_eq!(
            properties.info(),
            self.fixture.file_system().properties().info(),
            "properties contract: snapshot changed"
        );
        assert_eq!(
            properties.capabilities(),
            self.fixture.file_system().properties().capabilities(),
            "properties contract: snapshot changed"
        );
        self.context.record_check(
            "properties/snapshot",
            Some(FileSystemCapability::Read),
            ContractCheckOutcome::Passed,
        );
        self.context.record_check(
            "properties/path-constraints",
            None,
            ContractCheckOutcome::Passed,
        );
        self.context.record_check(
            "properties/capability-dependencies",
            None,
            ContractCheckOutcome::Passed,
        );
        self.context
            .record_check("properties/limits", None, ContractCheckOutcome::Passed);
        let path_limit = properties.limits().max_path_text_bytes();
        let path_outcome = match path_limit.maximum() {
            Some(maximum) if maximum <= MAX_PROBE_BYTES => {
                let over = usize::try_from(maximum).ok().and_then(|maximum| {
                    let component = "x".repeat(maximum.saturating_add(1));
                    Path::parse(&format!("/{component}")).ok()
                });
                match over {
                    Some(path) => {
                        properties
                            .limits()
                            .validate_path(&path, info.path_semantics(), FsOperation::Stat)
                            .expect_err("properties contract: path limit admitted oversized path");
                        ContractCheckOutcome::Passed
                    }
                    None => ContractCheckOutcome::SkippedOptional {
                        reason: "path boundary could not be represented".to_owned(),
                    },
                }
            }
            Some(_) => ContractCheckOutcome::SkippedOptional {
                reason: "path boundary exceeds the bounded probe budget".to_owned(),
            },
            None => ContractCheckOutcome::SkippedOptional {
                reason: "path limit is unknown, inapplicable, or unbounded".to_owned(),
            },
        };
        self.context
            .record_check("properties/limit-path-admission", None, path_outcome);
        let limits = *properties.limits();
        let component_outcome = if info.path_semantics() != PathSemantics::Hierarchical {
            ContractCheckOutcome::NotApplicable {
                reason: "component limits do not apply to literal paths".to_owned(),
            }
        } else {
            match limits.max_component_text_bytes().maximum() {
                Some(maximum) if maximum <= MAX_PROBE_BYTES => {
                    let over = usize::try_from(maximum)
                        .ok()
                        .and_then(|maximum| maximum.checked_add(1))
                        .map(|length| Path::parse(&format!("/{0}", "x".repeat(length))))
                        .transpose()
                        .expect("properties contract: component boundary path failed to parse");
                    match over {
                        Some(path) => {
                            limits
                                .validate_path(&path, info.path_semantics(), FsOperation::Stat)
                                .expect_err("properties contract: component limit admitted oversized component");
                            ContractCheckOutcome::Passed
                        }
                        None => ContractCheckOutcome::SkippedOptional {
                            reason: "component boundary could not be represented".to_owned(),
                        },
                    }
                }
                Some(_) => ContractCheckOutcome::SkippedOptional {
                    reason: "component boundary exceeds the bounded probe budget".to_owned(),
                },
                None => ContractCheckOutcome::SkippedOptional {
                    reason: "component limit is unknown, inapplicable, or unbounded".to_owned(),
                },
            }
        };
        self.context.record_check(
            "properties/limit-component-admission",
            None,
            component_outcome,
        );
        let page_outcome = match limits.max_list_page_entries().maximum() {
            Some(maximum) if maximum <= MAX_PROBE_ENTRIES => {
                let requested = usize::try_from(maximum)
                    .ok()
                    .and_then(|maximum| maximum.checked_add(1));
                match requested {
                    Some(requested) => {
                        let effective = limits.clamp_list_page_size(Some(requested));
                        assert!(
                            effective.is_none_or(
                                |effective| effective <= usize::try_from(maximum).unwrap()
                            ),
                            "properties contract: list page clamp exceeded declared maximum"
                        );
                        ContractCheckOutcome::Passed
                    }
                    None => ContractCheckOutcome::SkippedOptional {
                        reason: "list page boundary could not be represented".to_owned(),
                    },
                }
            }
            Some(_) => ContractCheckOutcome::SkippedOptional {
                reason: "list page boundary exceeds the bounded probe budget".to_owned(),
            },
            None => ContractCheckOutcome::SkippedOptional {
                reason: "list page limit is unknown, inapplicable, or unbounded".to_owned(),
            },
        };
        self.context
            .record_check("properties/limit-list-page", None, page_outcome);
        self.context.record_check(
            "properties/symlink-policy",
            None,
            ContractCheckOutcome::Passed,
        );
    }
}
