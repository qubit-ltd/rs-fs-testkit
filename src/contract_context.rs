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
use qubit_fs::directory::DeleteOptions;
use qubit_fs::error::FsErrorKind;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::FileSystemProperties;
use qubit_fs::path::Path;

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractReport;
use crate::FileSystemContract;
use crate::FixtureError;
use crate::internal::cleanup_failure::CleanupFailure;
use crate::internal::tracked_resource::TrackedResource;

/// Holds the immutable snapshot, report, and cleanup ledger for one run.
pub(crate) struct ContractContext {
    properties: FileSystemProperties,
    name_counter: u64,
    resources: Vec<TrackedResource>,
    current_contract: &'static str,
    pub(crate) run: crate::ContractRun,
    resource_index: u64,
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
            run: crate::ContractRun::new(),
            resource_index: 0,
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
            self.current_contract().replace('/', "-"),
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
    pub(crate) fn record_resource(&mut self, path: Path, owner_check: &'static str) {
        if self.resources.iter().any(|resource| resource.path == path) {
            return;
        }
        self.resource_index = self.resource_index.saturating_add(1);
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
        &self.run.report
    }

    /// Returns mutable access for a suite phase to record a check outcome.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn report_mut(&mut self) -> &mut ContractReport {
        &mut self.run.report
    }

    /// Records one stable check outcome after its phase has executed.
    #[inline]
    pub(crate) fn record_check(
        &mut self,
        id: ContractCheckId,
        capability: Option<FileSystemCapability>,
        outcome: ContractCheckOutcome,
    ) {
        self.run.report.record(id, capability, outcome);
    }

    /// Registers every check expected for a phase before it executes.
    pub(crate) fn prepare_phase(&mut self, contract: FileSystemContract, asynchronous: bool) {
        for spec in crate::internal::check_catalog::for_contract(contract, asynchronous) {
            self.run.report.register(spec.id, spec.capability);
        }
    }

    /// Records a failed check without erasing the original typed cause.
    pub(crate) fn fail(&mut self, failure: crate::ContractFailure) {
        if let Some(id) = failure.check() {
            self.run.report.record(
                id,
                crate::internal::check_catalog::specification(id).capability,
                ContractCheckOutcome::Failed {
                    reason: failure.message().to_owned(),
                },
            );
        }
        self.run.failures.push(failure);
    }

    /// Saves an observed failure before any later I/O can suspend.
    fn record_cleanup_failure(&mut self, failure: CleanupFailure) {
        self.run.cleanup.failures.push(crate::ContractFailure::with_source(
            format!(
                "[fs-testkit:cleanup/{}] owner={} path={:?}: {}",
                failure.operation, failure.owner_check, failure.path, failure.cause
            ),
            failure.cause,
        ));
    }

    /// Attempts every recorded synchronous resource and collects failures.
    ///
    /// Missing paths count as already cleaned. Failed resources remain in the
    /// ledger so a later explicit `finish` can retry them.
    pub(crate) fn cleanup(&mut self, file_system: &FileSystem) {
        if !self.properties.capabilities().supports(FileSystemCapability::Delete) {
            return;
        }
        let mut pending = std::mem::take(&mut self.resources);
        let mut retained = Vec::with_capacity(pending.len());
        while let Some(resource) = pending.pop() {
            let path = resource.path.clone();
            let owner = resource.owner_check;
            let inspected = catch_unwind(AssertUnwindSafe(|| file_system.stat(&path)));
            let metadata = match inspected {
                Ok(Ok(metadata)) => metadata,
                Ok(Err(error)) if error.kind() == FsErrorKind::NotFound => continue,
                Ok(Err(error)) => {
                    self.record_cleanup_failure(CleanupFailure {
                        owner_check: owner,
                        operation: "stat",
                        path: Some(path),
                        cause: FixtureError::with_source("facade cleanup operation failed", error),
                    });
                    retained.push(resource);
                    continue;
                }
                Err(payload) => {
                    self.record_cleanup_failure(CleanupFailure {
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
                    let options = if self
                        .properties
                        .capabilities()
                        .supports(FileSystemCapability::RecursiveDelete)
                    {
                        DeleteOptions::default().with_recursive(true)
                    } else {
                        DeleteOptions::default()
                    };
                    file_system.delete_directory(&path, options)
                } else {
                    file_system.delete_file(&path, Default::default())
                }
            }));
            match deleted {
                Ok(Ok(_outcome)) => {
                    let verified = catch_unwind(AssertUnwindSafe(|| file_system.stat(&path)));
                    match verified {
                        Ok(Err(error)) if error.kind() == FsErrorKind::NotFound => {}
                        Ok(Ok(_)) => {
                            self.record_cleanup_failure(CleanupFailure {
                                owner_check: owner,
                                operation: "delete",
                                path: Some(path),
                                cause: FixtureError::new("delete reported success but the resource still exists"),
                            });
                            retained.push(resource);
                        }
                        Ok(Err(error)) => {
                            self.record_cleanup_failure(CleanupFailure {
                                owner_check: owner,
                                operation: "delete",
                                path: Some(path),
                                cause: FixtureError::with_source("delete outcome could not be verified", error),
                            });
                            retained.push(resource);
                        }
                        Err(payload) => {
                            self.record_cleanup_failure(CleanupFailure {
                                owner_check: owner,
                                operation: "delete",
                                path: Some(path),
                                cause: panic_error(payload),
                            });
                            retained.push(resource);
                        }
                    }
                }
                Ok(Err(error)) if error.kind() == FsErrorKind::NotFound => {}
                Ok(Err(error)) => {
                    self.record_cleanup_failure(CleanupFailure {
                        owner_check: owner,
                        operation: "delete",
                        path: Some(path),
                        cause: FixtureError::with_source("facade cleanup operation failed", error),
                    });
                    retained.push(resource);
                }
                Err(payload) => {
                    self.record_cleanup_failure(CleanupFailure {
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
    }

    /// Attempts every recorded asynchronous resource and collects failures.
    ///
    /// Entries stay in the context across every await point. Cancellation may
    /// leave a deletion unconfirmed, but a subsequent attempt can observe
    /// `NotFound` and release that entry without losing any other resource.
    /// Failed entries remain in creation order and are retried by `finish`.
    #[cfg(feature = "async")]
    pub(crate) async fn cleanup_async(&mut self, file_system: &AsyncFileSystem) {
        if !self.properties.capabilities().supports(FileSystemCapability::Delete) {
            return;
        }
        for index in (0..self.resources.len()).rev() {
            let resource = &self.resources[index];
            let path = resource.path.clone();
            let owner = resource.owner_check;
            let inspected = crate::internal::catch_unwind_future(file_system.stat(&path)).await;
            let metadata = match inspected {
                Ok(Ok(metadata)) => metadata,
                Ok(Err(error)) if error.kind() == FsErrorKind::NotFound => {
                    self.resources.remove(index);
                    continue;
                }
                Ok(Err(error)) => {
                    self.record_cleanup_failure(CleanupFailure {
                        owner_check: owner,
                        operation: "stat",
                        path: Some(path),
                        cause: FixtureError::with_source("facade cleanup operation failed", error),
                    });
                    continue;
                }
                Err(payload) => {
                    self.record_cleanup_failure(CleanupFailure {
                        owner_check: owner,
                        operation: "stat",
                        path: Some(path),
                        cause: panic_error(payload),
                    });
                    continue;
                }
            };
            let deleted = if metadata.is_directory_like() {
                let options = if self
                    .properties
                    .capabilities()
                    .supports(FileSystemCapability::RecursiveDelete)
                {
                    DeleteOptions::default().with_recursive(true)
                } else {
                    DeleteOptions::default()
                };
                crate::internal::catch_unwind_future(file_system.delete_directory(&path, options)).await
            } else {
                crate::internal::catch_unwind_future(file_system.delete_file(&path, Default::default())).await
            };
            match deleted {
                Ok(Ok(_outcome)) => {
                    let verified = crate::internal::catch_unwind_future(file_system.stat(&path)).await;
                    match verified {
                        Ok(Err(error)) if error.kind() == FsErrorKind::NotFound => {
                            self.resources.remove(index);
                        }
                        Ok(Ok(_)) => {
                            self.record_cleanup_failure(CleanupFailure {
                                owner_check: owner,
                                operation: "delete",
                                path: Some(path),
                                cause: FixtureError::new("delete reported success but the resource still exists"),
                            });
                        }
                        Ok(Err(error)) => {
                            self.record_cleanup_failure(CleanupFailure {
                                owner_check: owner,
                                operation: "delete",
                                path: Some(path),
                                cause: FixtureError::with_source("delete outcome could not be verified", error),
                            });
                        }
                        Err(payload) => {
                            self.record_cleanup_failure(CleanupFailure {
                                owner_check: owner,
                                operation: "delete",
                                path: Some(path),
                                cause: panic_error(payload),
                            });
                        }
                    }
                }
                Ok(Err(error)) if error.kind() == FsErrorKind::NotFound => {
                    self.resources.remove(index);
                }
                Ok(Err(error)) => {
                    self.record_cleanup_failure(CleanupFailure {
                        owner_check: owner,
                        operation: "delete",
                        path: Some(path),
                        cause: FixtureError::with_source("facade cleanup operation failed", error),
                    });
                }
                Err(payload) => {
                    self.record_cleanup_failure(CleanupFailure {
                        owner_check: owner,
                        operation: "delete",
                        path: Some(path),
                        cause: panic_error(payload),
                    });
                }
            }
        }
    }
}

/// Converts an arbitrary provider panic into a safe fixture error.
fn panic_error(payload: Box<dyn Any + Send>) -> FixtureError {
    FixtureError::with_source(
        "provider panicked during cleanup",
        crate::ContractFailure::panicked("provider cleanup panic", payload),
    )
}
