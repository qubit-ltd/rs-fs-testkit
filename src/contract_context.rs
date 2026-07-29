// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow all -- contract behavior is covered by the conforming and
// fault matrices.
//! Mutable state shared by one contract-suite run.

use qubit_fs::{
    AsyncFileSystem, FileKind, FileSystem, FileSystemCapability, FileSystemProperties, FsErrorKind,
    Path,
};

/// Holds the immutable snapshot and mutable namespace bookkeeping for a suite.
pub(crate) struct ContractContext {
    properties: FileSystemProperties,
    name_counter: u64,
    created_paths: Vec<Path>,
    current_contract: &'static str,
}

impl ContractContext {
    /// Creates context from the facade's cached immutable property snapshot.
    pub(crate) fn new(properties: &FileSystemProperties) -> Self {
        Self {
            properties: properties.clone(),
            name_counter: 0,
            created_paths: Vec::new(),
            current_contract: "initialization",
        }
    }

    /// Returns the suite's single property snapshot.
    pub(crate) const fn properties(&self) -> &FileSystemProperties {
        &self.properties
    }

    /// Starts a named contract assertion and advances the unique-name counter.
    pub(crate) fn begin(&mut self, contract: &'static str) {
        self.current_contract = contract;
        self.name_counter = self.name_counter.saturating_add(1);
    }

    /// Returns a suite-unique relative name for the current contract phase.
    pub(crate) fn relative_name(&self, relative: &str) -> String {
        format!(
            "{}-{}-{}",
            self.current_contract, self.name_counter, relative
        )
    }

    /// Records a path created by the current contract assertion.
    pub(crate) fn record_created(&mut self, path: Path) {
        self.created_paths.push(path);
    }

    /// Returns the contract currently being evaluated for diagnostic context.
    pub(crate) const fn current_contract(&self) -> &'static str {
        self.current_contract
    }

    /// Removes resources recorded by completed contract phases in reverse
    /// order.
    ///
    /// Cleanup is best-effort: a resource that a phase already removed is not a
    /// contract failure, while any other cleanup error is reported with the
    /// phase that owned the resource. Providers without deletion capability
    /// retain the fixture-owned resources because the suite cannot clean them.
    pub(crate) fn cleanup(&mut self, file_system: &FileSystem) {
        if !self
            .properties
            .capabilities()
            .contains(FileSystemCapability::Delete)
        {
            self.created_paths.clear();
            return;
        }
        while let Some(path) = self.created_paths.pop() {
            let metadata = match file_system.stat(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == FsErrorKind::NotFound => continue,
                Err(error) => panic!(
                    "{} contract: cleanup stat failed for {path}: {error}",
                    self.current_contract
                ),
            };
            let result = if metadata.kind == FileKind::Directory {
                file_system.delete_directory(&path, Default::default())
            } else {
                file_system.delete_file(&path, Default::default())
            };
            result.unwrap_or_else(|error| {
                panic!(
                    "{} contract: cleanup failed for {path}: {error}",
                    self.current_contract
                )
            });
        }
    }

    /// Asynchronously removes resources recorded by completed contract phases.
    ///
    /// Cleanup follows the synchronous suite semantics: missing resources are
    /// already cleaned, while every other observation or deletion error fails
    /// the current contract with its diagnostic context.
    pub(crate) async fn cleanup_async(&mut self, file_system: &AsyncFileSystem) {
        if !self
            .properties
            .capabilities()
            .contains(FileSystemCapability::Delete)
        {
            self.created_paths.clear();
            return;
        }
        while let Some(path) = self.created_paths.pop() {
            let metadata = match file_system.stat(&path).await {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == FsErrorKind::NotFound => continue,
                Err(error) => panic!(
                    "{} contract: cleanup stat failed for {path}: {error}",
                    self.current_contract
                ),
            };
            let result = if metadata.kind == FileKind::Directory {
                file_system
                    .delete_directory(&path, Default::default())
                    .await
            } else {
                file_system.delete_file(&path, Default::default()).await
            };
            result.unwrap_or_else(|error| {
                panic!(
                    "{} contract: cleanup failed for {path}: {error}",
                    self.current_contract
                )
            });
        }
    }
}
