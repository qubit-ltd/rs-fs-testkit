// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Mutable state shared by one contract-suite run.

#[cfg(feature = "async")]
use qubit_fs::AsyncFileSystem;
use qubit_fs::FileSystem;
use qubit_fs::FileSystemCapability;
use qubit_fs::FileSystemProperties;
use qubit_fs::FsErrorKind;
use qubit_fs::Path;

/// Holds the immutable snapshot and mutable namespace bookkeeping for a suite.
pub(crate) struct ContractContext {
    /// Immutable property snapshot captured when the suite starts.
    properties: FileSystemProperties,
    /// Counter used to make repeated contract phase names unique.
    name_counter: u64,
    /// Paths retained for best-effort cleanup in reverse creation order.
    created_paths: Vec<Path>,
    /// Name of the contract phase currently producing diagnostics.
    current_contract: &'static str,
}

impl ContractContext {
    /// Creates context from the facade's cached immutable property snapshot.
    ///
    /// # Parameters
    ///
    /// * `properties` - Property snapshot exposed by the tested facade.
    ///
    /// # Returns
    ///
    /// Fresh context in the initialization phase with no recorded paths.
    #[inline]
    pub(crate) fn new(properties: &FileSystemProperties) -> Self {
        Self {
            properties: properties.clone(),
            name_counter: 0,
            created_paths: Vec::new(),
            current_contract: "initialization",
        }
    }

    /// Returns the suite's single property snapshot.
    ///
    /// # Returns
    ///
    /// The immutable snapshot captured when the context was created.
    #[inline(always)]
    pub(crate) const fn properties(&self) -> &FileSystemProperties {
        &self.properties
    }

    /// Starts a named contract assertion and advances the unique-name counter.
    ///
    /// # Parameters
    ///
    /// * `contract` - Static contract name used in paths and diagnostics.
    #[inline]
    pub(crate) fn begin(&mut self, contract: &'static str) {
        self.current_contract = contract;
        self.name_counter = self.name_counter.saturating_add(1);
    }

    /// Returns a suite-unique relative name for the current contract phase.
    ///
    /// # Parameters
    ///
    /// * `relative` - Human-readable suffix describing the contract resource.
    ///
    /// # Returns
    ///
    /// A name containing the current contract, invocation counter, and suffix.
    #[inline]
    pub(crate) fn relative_name(&self, relative: &str) -> String {
        format!(
            "{}-{}-{}",
            self.current_contract(),
            self.name_counter,
            relative
        )
    }

    /// Records a path created by the current contract assertion.
    ///
    /// # Parameters
    ///
    /// * `path` - Provider path that cleanup should remove later.
    #[inline]
    pub(crate) fn record_created(&mut self, path: Path) {
        self.created_paths.push(path);
    }

    /// Returns the contract currently being evaluated for diagnostic context.
    ///
    /// # Returns
    ///
    /// The static name most recently supplied to [`Self::begin`].
    #[inline(always)]
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
    ///
    /// # Parameters
    ///
    /// * `file_system` - Synchronous facade used to inspect and delete paths.
    ///
    /// # Panics
    ///
    /// Panics when metadata inspection or deletion fails for a recorded path,
    /// except when inspection reports that the path is already absent.
    pub(crate) fn cleanup(&mut self, file_system: &FileSystem) {
        if !self
            .properties
            .capabilities()
            .supports(FileSystemCapability::Delete)
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
            let result = if metadata.is_directory_like() {
                file_system.delete_directory(&path, Default::default())
            } else {
                file_system.delete_file(&path, Default::default())
            };
            result.expect("contract cleanup failed");
        }
    }

    /// Asynchronously removes resources recorded by completed contract phases.
    ///
    /// Cleanup follows the synchronous suite semantics: missing resources are
    /// already cleaned, while every other observation or deletion error fails
    /// the current contract with its diagnostic context.
    ///
    /// # Parameters
    ///
    /// * `file_system` - Asynchronous facade used to inspect and delete paths.
    ///
    /// # Panics
    ///
    /// Panics when metadata inspection or deletion fails for a recorded path,
    /// except when inspection reports that the path is already absent.
    #[cfg(feature = "async")]
    pub(crate) async fn cleanup_async(
        &mut self,
        file_system: &AsyncFileSystem,
    ) {
        if !self
            .properties
            .capabilities()
            .supports(FileSystemCapability::Delete)
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
            let result = if metadata.is_directory_like() {
                file_system
                    .delete_directory(&path, Default::default())
                    .await
            } else {
                file_system.delete_file(&path, Default::default()).await
            };
            result.expect("contract cleanup failed");
        }
    }
}
