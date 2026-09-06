// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Mutable state shared by one contract-suite run.

use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;

#[cfg(feature = "async")]
use qubit_fs::AsyncFileSystem;
use qubit_fs::FileSystem;
use qubit_fs::error::FsErrorKind;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::FileSystemProperties;
use qubit_fs::path::Path;

use crate::ContractCheckOutcome;
use crate::ContractReport;
use crate::FixtureError;
use crate::internal::cleanup_failure::CleanupFailure;
use crate::internal::tracked_resource::TrackedResource;
use crate::FileSystemContract;

/// Holds the immutable snapshot, report, and cleanup ledger for one run.
pub(crate) struct ContractContext {
    properties: FileSystemProperties,
    name_counter: u64,
    resources: Vec<TrackedResource>,
    current_contract: &'static str,
    report: ContractReport,
    resource_index: u64,
    resources_prepared: bool,
}

impl ContractContext {
    /// Creates context from the facade's cached immutable property snapshot.
    #[inline]
    pub(crate) fn new(properties: &FileSystemProperties) -> Self {
        Self {
            properties: properties.clone(),
            name_counter: 0,
            resources: Vec::new(),
            current_contract: "initialization",
            report: ContractReport::new(),
            resource_index: 0,
            resources_prepared: false,
        }
    }

    /// Returns the suite's immutable property snapshot.
    #[inline(always)]
    pub(crate) const fn properties(&self) -> &FileSystemProperties {
        &self.properties
    }

    /// Starts a named contract assertion and advances the unique-name counter.
    #[inline]
    pub(crate) fn begin(&mut self, contract: &'static str) {
        self.current_contract = contract;
        self.name_counter = self.name_counter.saturating_add(1);
    }

    /// Returns a suite-unique relative name for the current contract phase.
    #[inline]
    pub(crate) fn relative_name(&self, relative: &str) -> String {
        format!(
            "{}-{}-{}",
            self.current_contract(),
            self.name_counter,
            relative
        )
    }

    /// Records a path with its owning check, retaining only the first owner.
    #[inline]
    pub(crate) fn record_created(&mut self, path: Path) {
        self.record_resource(path, self.current_contract);
    }

    /// Records a path with an explicit check owner.
    #[inline]
    pub(crate) fn record_resource(
        &mut self,
        path: Path,
        owner_check: &'static str,
    ) {
        if self.resources.iter().any(|resource| resource.path == path) {
            return;
        }
        self.resource_index = self.resource_index.saturating_add(1);
        self.resources_prepared = true;
        self.resources.push(TrackedResource {
            path,
            owner_check,
            creation_index: self.resource_index,
        });
    }

    /// Returns the contract currently producing diagnostics.
    #[inline(always)]
    pub(crate) const fn current_contract(&self) -> &'static str {
        self.current_contract
    }

    /// Returns the report accumulated by the suite.
    #[inline]
    pub(crate) const fn report(&self) -> &ContractReport {
        &self.report
    }

    /// Returns mutable access for a suite phase to record a check outcome.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn report_mut(&mut self) -> &mut ContractReport {
        &mut self.report
    }

    /// Returns whether this run acquired any fixture-owned resource.
    #[inline]
    pub(crate) const fn resources_prepared(&self) -> bool {
        self.resources_prepared
    }

    /// Records one stable check outcome after its phase has executed.
    #[inline]
    pub(crate) fn record_check(
        &mut self,
        id: &'static str,
        capability: Option<FileSystemCapability>,
        outcome: ContractCheckOutcome,
    ) {
        self.report.record(id, capability, outcome);
    }

    /// Registers every check expected for a phase before it executes.
    pub(crate) fn prepare_phase(&mut self, contract: FileSystemContract) {
        for spec in crate::internal::check_catalog::for_contract(contract) {
            self.report.expect(spec.id);
            self.report.record(
                spec.id,
                spec.capability,
                ContractCheckOutcome::Unverified {
                    reason: "check not yet executed".to_owned(),
                },
            );
        }
    }

    /// Attempts every recorded synchronous resource and collects failures.
    ///
    /// Missing paths count as already cleaned. Failed resources remain in the
    /// ledger so a later explicit `finish` can retry them.
    pub(crate) fn cleanup(&mut self, file_system: &FileSystem) -> Vec<CleanupFailure> {
        if !self
            .properties
            .capabilities()
            .supports(FileSystemCapability::Delete)
        {
            return Vec::new();
        }
        let mut pending = std::mem::take(&mut self.resources);
        let mut retained = Vec::with_capacity(pending.len());
        let mut failures = Vec::new();
        while let Some(resource) = pending.pop() {
            let path = resource.path.clone();
            let owner = resource.owner_check;
            let inspected = catch_unwind(AssertUnwindSafe(|| file_system.stat(&path)));
            let metadata = match inspected {
                Ok(Ok(metadata)) => metadata,
                Ok(Err(error)) if error.kind() == FsErrorKind::NotFound => continue,
                Ok(Err(error)) => {
                    failures.push(CleanupFailure {
                        owner_check: owner,
                        operation: "stat",
                        path: Some(path),
                        cause: FixtureError::new(error.to_string()),
                    });
                    retained.push(resource);
                    continue;
                }
                Err(payload) => {
                    failures.push(CleanupFailure {
                        owner_check: owner,
                        operation: "stat",
                        path: Some(path),
                        cause: panic_error(payload),
                    });
                    retained.push(resource);
                    continue;
                }
            };
            let deleted = catch_unwind(AssertUnwindSafe(|| {
                if metadata.is_directory_like() {
                    file_system.delete_directory(&path, Default::default())
                } else {
                    file_system.delete_file(&path, Default::default())
                }
            }));
            match deleted {
                Ok(Ok(_)) => {}
                Ok(Err(error)) if error.kind() == FsErrorKind::NotFound => {}
                Ok(Err(error)) => {
                    failures.push(CleanupFailure {
                        owner_check: owner,
                        operation: "delete",
                        path: Some(path),
                        cause: FixtureError::new(error.to_string()),
                    });
                    retained.push(resource);
                }
                Err(payload) => {
                    failures.push(CleanupFailure {
                        owner_check: owner,
                        operation: "delete",
                        path: Some(path),
                        cause: panic_error(payload),
                    });
                    retained.push(resource);
                }
            }
        }
        retained.sort_by_key(|resource| resource.creation_index);
        self.resources = retained;
        failures
    }

    /// Attempts every recorded asynchronous resource and collects failures.
    #[cfg(feature = "async")]
    pub(crate) async fn cleanup_async(
        &mut self,
        file_system: &AsyncFileSystem,
    ) -> Vec<CleanupFailure> {
        if !self
            .properties
            .capabilities()
            .supports(FileSystemCapability::Delete)
        {
            return Vec::new();
        }
        let mut pending = std::mem::take(&mut self.resources);
        let mut retained = Vec::with_capacity(pending.len());
        let mut failures = Vec::new();
        while let Some(resource) = pending.pop() {
            let path = resource.path.clone();
            let owner = resource.owner_check;
            let inspected = crate::internal::catch_unwind_future(file_system.stat(&path)).await;
            let metadata = match inspected {
                Ok(Ok(metadata)) => metadata,
                Ok(Err(error)) if error.kind() == FsErrorKind::NotFound => continue,
                Ok(Err(error)) => {
                    failures.push(CleanupFailure {
                        owner_check: owner,
                        operation: "stat",
                        path: Some(path),
                        cause: FixtureError::new(error.to_string()),
                    });
                    retained.push(resource);
                    continue;
                }
                Err(payload) => {
                    failures.push(CleanupFailure {
                        owner_check: owner,
                        operation: "stat",
                        path: Some(path),
                        cause: panic_error(payload),
                    });
                    retained.push(resource);
                    continue;
                }
            };
            let deleted = if metadata.is_directory_like() {
                crate::internal::catch_unwind_future(
                    file_system.delete_directory(&path, Default::default()),
                )
                .await
            } else {
                crate::internal::catch_unwind_future(
                    file_system.delete_file(&path, Default::default()),
                )
                .await
            };
            match deleted {
                Ok(Ok(_)) => {}
                Ok(Err(error)) if error.kind() == FsErrorKind::NotFound => {}
                Ok(Err(error)) => {
                    failures.push(CleanupFailure {
                        owner_check: owner,
                        operation: "delete",
                        path: Some(path),
                        cause: FixtureError::new(error.to_string()),
                    });
                    retained.push(resource);
                }
                Err(payload) => {
                    failures.push(CleanupFailure {
                        owner_check: owner,
                        operation: "delete",
                        path: Some(path),
                        cause: panic_error(payload),
                    });
                    retained.push(resource);
                }
            }
        }
        retained.sort_by_key(|resource| resource.creation_index);
        self.resources = retained;
        failures
    }
}

/// Converts an arbitrary provider panic into a safe fixture error.
fn panic_error(payload: Box<dyn Any + Send>) -> FixtureError {
    if let Some(message) = payload.downcast_ref::<String>() {
        FixtureError::new(message.clone())
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        FixtureError::new(*message)
    } else {
        FixtureError::new("non-string provider panic during cleanup")
    }
}
