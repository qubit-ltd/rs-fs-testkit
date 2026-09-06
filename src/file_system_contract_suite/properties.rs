// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements property snapshots and bounded limit checks.

use super::*;
use crate::internal::limit_probe_plan::MAX_PROBE_BYTES;

impl<'a> FileSystemContractSuite<'a> {
    /// Checks immutable facade properties and fixture path compatibility.
    ///
    /// # Panics
    ///
    /// Panics when identifiers or capabilities are inconsistent, the fixture
    /// path is invalid, or the facade snapshot changes during the suite run.
    pub fn assert_properties(&mut self) {
        self.context.begin("properties");
        let properties = self.context.properties();
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
        assert_eq!(
            properties.limits(),
            self.fixture.file_system().properties().limits(),
            "properties contract: limits snapshot changed"
        );
        assert_eq!(
            properties.path_constraints(),
            self.fixture.file_system().properties().path_constraints(),
            "properties contract: path constraints snapshot changed"
        );
        assert_eq!(
            properties.symlink_policy(),
            self.fixture.file_system().properties().symlink_policy(),
            "properties contract: symlink policy snapshot changed"
        );
        let limits = *properties.limits();
        let path_limit = limits.max_path_text_bytes();
        let path_semantics = info.path_semantics();
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
        self.context.record_check(
            "properties/limits",
            None,
            ContractCheckOutcome::Passed,
        );
        let path_outcome = match path_limit.maximum() {
            Some(maximum) if maximum <= MAX_PROBE_BYTES => {
                let over = usize::try_from(maximum)
                    .ok()
                    .and_then(|maximum| {
                        let component = "x".repeat(maximum.saturating_add(1));
                        Path::parse(&format!("/{component}")).ok()
                    });
                match over {
                    Some(path) => {
                        limits.validate_path(
                                &path,
                                path_semantics,
                                FsOperation::Stat,
                            )
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
        self.context.record_check(
            "properties/limit-path-admission",
            None,
            path_outcome,
        );
        self.context.record_check(
            "properties/symlink-policy",
            None,
            ContractCheckOutcome::Passed,
        );
    }
}
