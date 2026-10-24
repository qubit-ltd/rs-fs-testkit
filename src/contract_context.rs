// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
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

/// Holds the immutable filesystem snapshot, accumulated report, and cleanup
/// ledger for one contract-suite run.
pub(crate) struct ContractContext {
    /// Immutable capabilities and limits captured when the suite starts.
    properties: FileSystemProperties,
    /// Counter used to make names unique within the current run.
    name_counter: u64,
    /// Resources still owned by the suite's cleanup process.
    resources: Vec<TrackedResource>,
    /// Check name attached to newly created diagnostic paths.
    current_contract: &'static str,
    /// Report, cleanup history, and failures accumulated during this run.
    pub(crate) run: crate::ContractRun,
    /// Monotonic creation order assigned to tracked resources.
    resource_index: u64,
}

impl ContractContext {
    /// Creates context from the facade's cached immutable property snapshot.
    ///
    /// # Parameters
    ///
    /// `properties` is the immutable property snapshot already read by the
    /// facade.
    ///
    /// # Returns
    ///
    /// A context initialized for a new contract-suite run.
    #[must_use]
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
    #[must_use = "the filesystem property snapshot must be inspected"]
    #[inline]
    pub(crate) const fn properties(&self) -> &FileSystemProperties {
        &self.properties
    }

    /// Returns a suite-unique relative name for the current contract phase.
    #[must_use]
    #[inline]
    pub(crate) fn relative_name(&self, relative: &str) -> String {
        format!(
            "{}-{}-{}",
            self.current_contract().replace('/', "-"),
            self.name_counter,
            relative
        )
    }

    /// Returns the contract currently producing diagnostics.
    ///
    /// # Returns
    ///
    /// The static contract name currently assigned to this context.
    #[must_use]
    #[inline]
    pub(crate) const fn current_contract(&self) -> &'static str {
        self.current_contract
    }

    /// Returns the report accumulated by the suite.
    ///
    /// # Returns
    ///
    /// The report borrowed from the run state.
    #[must_use = "inspect the report when deciding whether the suite passed"]
    #[inline]
    pub(crate) const fn report(&self) -> &ContractReport {
        &self.run.report
    }

    /// Starts a named contract assertion and advances the unique-name counter.
    ///
    /// # Parameters
    ///
    /// `contract` is the static name attributed to subsequent diagnostics and
    /// resources.
    #[inline]
    pub(crate) fn begin(&mut self, contract: &'static str) {
        self.current_contract = contract;
        self.name_counter = self.name_counter.saturating_add(1);
    }

    /// Records a path with its owning check, retaining only the first owner.
    ///
    /// # Parameters
    ///
    /// `path` is the resource path created by the current contract.
    #[inline]
    pub(crate) fn record_created(&mut self, path: Path) {
        self.record_resource(path, self.current_contract);
    }

    /// Records a path with an explicit check owner.
    ///
    /// # Parameters
    ///
    /// `path` is the resource path to retain for cleanup.
    /// `owner_check` is the static check name responsible for that resource.
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

    /// Records one stable check outcome after its phase has executed.
    ///
    /// # Parameters
    ///
    /// `id` identifies the check, `capability` records its required capability,
    /// and `outcome` is the observed result.
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
    ///
    /// # Parameters
    ///
    /// `contract` selects the phase's checks. `asynchronous` selects the
    /// asynchronous check catalog when true.
    pub(crate) fn prepare_phase(&mut self, contract: FileSystemContract, asynchronous: bool) {
        for spec in crate::internal::check_catalog::for_contract(contract, asynchronous) {
            self.run.report.register(spec.id, spec.capability);
        }
    }

    /// Records a failed check without erasing the original typed cause.
    ///
    /// # Parameters
    ///
    /// `failure` is the diagnostic and optional check attribution to retain.
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

    /// Attempts every recorded synchronous resource and collects failures.
    ///
    /// Missing paths count as already cleaned. Failed resources remain in the
    /// ledger so a later explicit `finish` can retry them.
    ///
    /// # Parameters
    ///
    /// `file_system` performs the stat and deletion operations for each
    /// resource.
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
    ///
    /// # Parameters
    ///
    /// `file_system` performs the asynchronous stat and deletion operations.
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
}

/// Converts an arbitrary provider panic into a safe fixture error.
///
/// # Parameters
///
/// `payload` is the panic payload returned by the provider operation.
///
/// # Returns
///
/// A fixture error that retains the panic as a typed source.
fn panic_error(payload: Box<dyn Any + Send>) -> FixtureError {
    FixtureError::with_source(
        "provider panicked during cleanup",
        crate::ContractFailure::panicked("provider cleanup panic", payload),
    )
}

#[cfg(test)]
mod tests {
    use super::ContractContext;
    use crate::ContractFailure;
    use crate::FileSystemFixture;
    use crate::FixtureSupport;
    use crate::common::MemoryFault;
    use crate::common::MemoryFixture;

    #[test]
    fn test_counters_saturate_and_duplicate_resources_keep_the_first_owner() {
        use qubit_fs::path::Path;

        let fixture = MemoryFixture::new();
        let mut context = ContractContext::new(fixture.file_system().properties());
        context.name_counter = u64::MAX;
        context.begin("write/basic");
        assert_eq!(
            context.relative_name("payload"),
            "write-basic-18446744073709551615-payload"
        );

        let first = Path::parse("/context/first").expect("valid path");
        context.record_resource(first.clone(), "write/basic");
        context.record_resource(first, "copy/basic");
        assert_eq!(context.resources.len(), 1);
        assert_eq!(context.resources[0].owner_check, "write/basic");

        context.resource_index = u64::MAX;
        context.record_resource(Path::parse("/context/second").expect("valid path"), "copy/basic");
        assert_eq!(context.resource_index, u64::MAX);
        assert_eq!(context.resources.len(), 2);
    }

    #[test]
    fn test_created_resources_inherit_the_current_contract_owner() {
        use qubit_fs::path::Path;

        let fixture = MemoryFixture::new();
        let mut context = ContractContext::new(fixture.file_system().properties());
        context.begin("copy/tree");
        let path = Path::parse("/context/created").expect("valid path");

        context.record_created(path.clone());

        assert_eq!(context.resources.len(), 1);
        assert_eq!(context.resources[0].path, path);
        assert_eq!(context.resources[0].owner_check, "copy/tree");
        assert_eq!(context.resources[0].creation_index, 1);
    }

    #[test]
    fn test_fail_without_check_keeps_the_failure_without_changing_check_outcomes() {
        let fixture = MemoryFixture::new();
        let mut context = ContractContext::new(fixture.file_system().properties());
        let outcomes_before = context.report().checks().len();

        context.fail(ContractFailure::message_only("unattributed failure"));

        assert_eq!(context.run.failures.len(), 1);
        assert_eq!(context.run.failures[0].check(), None);
        assert_eq!(context.report().checks().len(), outcomes_before);
    }

    #[test]
    fn test_phase_registration_and_attributed_failure_share_one_report() {
        use crate::ContractCheckId;
        use crate::ContractCheckOutcome;
        use crate::FileSystemContract;

        let fixture = MemoryFixture::new();
        let mut context = ContractContext::new(fixture.file_system().properties());
        context.begin("read/basic");
        assert_eq!(context.current_contract(), "read/basic");
        assert!(context.relative_name("source").starts_with("read-basic-1-"));

        context.prepare_phase(FileSystemContract::Read, false);
        let check = context
            .report()
            .checks()
            .iter()
            .find(|check| check.id() == ContractCheckId::ReadBasic)
            .expect("read check registered before execution");
        assert!(matches!(check.outcome(), ContractCheckOutcome::NotRun { .. }));

        context.fail(ContractFailure::message_only("read evidence failed").at(ContractCheckId::ReadBasic));
        assert_eq!(context.run.failures.len(), 1);
        assert_eq!(context.run.failures[0].check(), Some(ContractCheckId::ReadBasic));
        let recorded = context
            .report()
            .checks()
            .iter()
            .find(|check| check.id() == ContractCheckId::ReadBasic)
            .expect("failed check remains registered");
        assert!(matches!(recorded.outcome(), ContractCheckOutcome::Failed { .. }));
    }

    #[cfg(feature = "async")]
    #[test]
    fn test_asynchronous_phase_registration_includes_async_only_checks() {
        use crate::ContractCheckId;
        use crate::FileSystemContract;

        let fixture = MemoryFixture::new();
        let mut context = ContractContext::new(fixture.file_system().properties());
        context.prepare_phase(FileSystemContract::Copy, true);

        let checks = context.report().checks();
        assert!(!checks.is_empty());
        assert!(checks.iter().any(|check| check.id() == ContractCheckId::CopyBasic));
        assert!(
            checks
                .iter()
                .any(|check| check.id() == ContractCheckId::AsyncCopyCancelCommit)
        );
    }

    #[test]
    fn test_cleanup_retains_resource_after_stat_panics_and_retries() {
        let fixture = MemoryFixture::new();
        let FixtureSupport::Supported(path) = fixture
            .seed_file("cleanup-stat-panic", b"payload")
            .expect("seed succeeds")
        else {
            panic!("memory fixture supports seeded files");
        };
        let mut context = ContractContext::new(fixture.file_system().properties());
        context.record_resource(path, "write/basic");

        fixture.set_fault(MemoryFault::CleanupStatPanic);
        context.cleanup(fixture.file_system());
        assert_eq!(context.resources.len(), 1);
        assert_eq!(context.run.cleanup.failures.len(), 1);

        fixture.set_fault(MemoryFault::None);
        context.cleanup(fixture.file_system());
        assert!(context.resources.is_empty());
        assert_eq!(context.run.cleanup.failures.len(), 1);
        assert!(fixture.is_empty());
    }

    #[test]
    fn test_cleanup_stat_errors_preserve_owner_path_and_source() {
        use crate::common::MemoryFault;

        let fixture = MemoryFixture::with_fault(MemoryFault::CleanupStatError);
        let FixtureSupport::Supported(path) = fixture
            .seed_file("cleanup-stat-error", b"payload")
            .expect("seed succeeds")
        else {
            panic!("memory fixture supports seeded files");
        };
        let mut context = ContractContext::new(fixture.file_system().properties());
        context.record_resource(path.clone(), "write/basic");

        context.cleanup(fixture.file_system());

        assert_eq!(context.resources.len(), 1);
        let failure = &context.run.cleanup.failures[0];
        let message = failure.message();
        assert!(message.contains("owner=write/basic"));
        assert!(message.contains(path.as_str()));
        assert!(message.contains("stat"));
        assert!(std::error::Error::source(failure).is_some());
    }

    #[test]
    fn test_cleanup_continues_after_a_resource_stat_failure() {
        use crate::common::MemoryFault;

        let fixture = MemoryFixture::with_fault(MemoryFault::CleanupStatError);
        let FixtureSupport::Supported(first) = fixture
            .seed_file("cleanup-first", b"first")
            .expect("first seed succeeds")
        else {
            panic!("memory fixture supports seeded files");
        };
        let FixtureSupport::Supported(second) = fixture
            .seed_file("cleanup-second", b"second")
            .expect("second seed succeeds")
        else {
            panic!("memory fixture supports seeded files");
        };
        let mut context = ContractContext::new(fixture.file_system().properties());
        context.record_resource(first, "write/first");
        context.record_resource(second, "write/second");

        context.cleanup(fixture.file_system());

        assert_eq!(context.resources.len(), 2);
        assert_eq!(context.run.cleanup.failures.len(), 2);
        assert!(!fixture.is_empty());
        fixture.set_fault(MemoryFault::None);
        context.cleanup(fixture.file_system());
        assert!(context.resources.is_empty());
        assert!(fixture.is_empty());
    }

    #[test]
    fn test_cleanup_retains_resource_when_post_delete_stat_errors_or_panics() {
        use crate::common::MemoryFault;

        for fault in [MemoryFault::CleanupVerifyStatError, MemoryFault::CleanupVerifyStatPanic] {
            let fixture = MemoryFixture::with_fault(fault);
            let FixtureSupport::Supported(path) = fixture
                .seed_file("cleanup-verify-stat", b"payload")
                .expect("seed succeeds")
            else {
                panic!("memory fixture supports seeded files");
            };
            let mut context = ContractContext::new(fixture.file_system().properties());
            context.record_resource(path, "write/basic");

            context.cleanup(fixture.file_system());

            assert_eq!(context.resources.len(), 1, "{fault:?}");
            assert_eq!(context.run.cleanup.failures.len(), 1, "{fault:?}");
            assert!(fixture.is_empty(), "deletion succeeded before verification failed");
        }
    }

    #[test]
    fn test_cleanup_retains_resource_after_delete_errors_and_noop() {
        use crate::common::MemoryFault;

        for fault in [
            MemoryFault::CleanupDeleteError,
            MemoryFault::CleanupDeletePanic,
            MemoryFault::DeleteNoOp,
        ] {
            let fixture = MemoryFixture::with_fault(fault);
            let FixtureSupport::Supported(path) = fixture
                .seed_file("cleanup-delete-failure", b"payload")
                .expect("seed succeeds")
            else {
                panic!("memory fixture supports seeded files");
            };
            let mut context = ContractContext::new(fixture.file_system().properties());
            context.record_resource(path, "write/basic");

            context.cleanup(fixture.file_system());

            assert_eq!(context.resources.len(), 1, "{fault:?}");
            assert_eq!(context.run.cleanup.failures.len(), 1, "{fault:?}");
            assert!(!fixture.is_empty(), "{fault:?}");
        }
    }

    #[test]
    fn test_cleanup_skips_unsupported_delete_and_releases_missing_paths() {
        use qubit_fs::path::Path;

        let fixture = MemoryFixture::without_delete();
        let mut context = ContractContext::new(fixture.file_system().properties());
        context.record_resource(Path::parse("/cleanup/unsupported").expect("valid path"), "write/basic");

        context.cleanup(fixture.file_system());

        assert_eq!(context.resources.len(), 1, "unsupported cleanup retains ownership");
        assert!(context.run.cleanup.failures.is_empty());

        let fixture = MemoryFixture::new();
        let mut context = ContractContext::new(fixture.file_system().properties());
        context.record_resource(
            Path::parse("/cleanup/already-missing").expect("valid path"),
            "write/basic",
        );

        context.cleanup(fixture.file_system());

        assert!(context.resources.is_empty(), "missing paths are already cleaned");
        assert!(context.run.cleanup.failures.is_empty());
    }

    #[test]
    fn test_cleanup_deletes_recorded_directories() {
        let fixture = MemoryFixture::new();
        let FixtureSupport::Supported(path) = fixture
            .seed_empty_directory("cleanup-recorded-directory")
            .expect("directory seed succeeds")
        else {
            panic!("memory fixture supports seeded directories");
        };
        let mut context = ContractContext::new(fixture.file_system().properties());
        context.record_resource(path, "directory/create");

        context.cleanup(fixture.file_system());

        assert!(context.resources.is_empty());
        assert!(context.run.cleanup.failures.is_empty());
        assert!(fixture.is_empty());
    }

    #[cfg(feature = "async")]
    #[test]
    fn test_async_cleanup_retains_resource_after_stat_panics_and_retries() {
        use crate::AsyncFileSystemFixture;
        use crate::common::AsyncMemoryFixture;
        use crate::common::async_memory_file_system::run_controlled;

        let fixture = AsyncMemoryFixture::new();
        run_controlled(async {
            let FixtureSupport::Supported(path) = fixture
                .seed_file("async-cleanup-stat-panic", b"payload")
                .await
                .expect("seed succeeds")
            else {
                panic!("memory fixture supports seeded files");
            };
            let mut context = ContractContext::new(fixture.file_system().properties());
            context.record_resource(path, "write/basic");

            fixture.set_cleanup_stat_panics(true);
            context.cleanup_async(fixture.file_system()).await;
            assert_eq!(context.resources.len(), 1);
            assert_eq!(context.run.cleanup.failures.len(), 1);

            fixture.set_cleanup_stat_panics(false);
            context.cleanup_async(fixture.file_system()).await;
            assert!(context.resources.is_empty());
            assert_eq!(context.run.cleanup.failures.len(), 1);
            assert!(fixture.is_empty());
        });
    }

    #[cfg(feature = "async")]
    #[test]
    fn test_async_cleanup_stat_errors_preserve_owner_path_and_source() {
        use crate::AsyncFileSystemFixture;
        use crate::common::AsyncMemoryFault;
        use crate::common::AsyncMemoryFixture;
        use crate::common::async_memory_file_system::run_controlled;

        let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::CleanupStatError);
        run_controlled(async {
            let FixtureSupport::Supported(path) = fixture
                .seed_file("async-cleanup-stat-error", b"payload")
                .await
                .expect("seed succeeds")
            else {
                panic!("memory fixture supports seeded files");
            };
            let mut context = ContractContext::new(fixture.file_system().properties());
            context.record_resource(path.clone(), "write/basic");

            context.cleanup_async(fixture.file_system()).await;

            assert_eq!(context.resources.len(), 1);
            let failure = &context.run.cleanup.failures[0];
            let message = failure.message();
            assert!(message.contains("owner=write/basic"));
            assert!(message.contains(path.as_str()));
            assert!(message.contains("stat"));
            assert!(std::error::Error::source(failure).is_some());
        });
    }

    #[cfg(feature = "async")]
    #[test]
    fn test_async_cleanup_retains_resource_when_post_delete_stat_errors_or_panics() {
        use crate::AsyncFileSystemFixture;
        use crate::common::AsyncMemoryFault;
        use crate::common::AsyncMemoryFixture;
        use crate::common::async_memory_file_system::run_controlled;

        for fault in [
            AsyncMemoryFault::CleanupVerifyStatError,
            AsyncMemoryFault::CleanupVerifyStatPanic,
        ] {
            let fixture = AsyncMemoryFixture::with_fault(fault);
            run_controlled(async {
                let FixtureSupport::Supported(path) = fixture
                    .seed_file("async-cleanup-verify-stat", b"payload")
                    .await
                    .expect("seed succeeds")
                else {
                    panic!("memory fixture supports seeded files");
                };
                let mut context = ContractContext::new(fixture.file_system().properties());
                context.record_resource(path, "write/basic");

                context.cleanup_async(fixture.file_system()).await;

                assert_eq!(context.resources.len(), 1, "{fault:?}");
                assert_eq!(context.run.cleanup.failures.len(), 1, "{fault:?}");
                assert!(fixture.is_empty(), "deletion succeeded before verification failed");
            });
        }
    }

    #[cfg(feature = "async")]
    #[test]
    fn test_async_cleanup_retains_resource_after_delete_errors_and_noop() {
        use crate::AsyncFileSystemFixture;
        use crate::common::AsyncMemoryFault;
        use crate::common::AsyncMemoryFixture;
        use crate::common::async_memory_file_system::run_controlled;

        for fault in [
            AsyncMemoryFault::CleanupDeleteError,
            AsyncMemoryFault::CleanupDeletePanic,
            AsyncMemoryFault::DeleteNoOp,
        ] {
            let fixture = AsyncMemoryFixture::with_fault(fault);
            run_controlled(async {
                let FixtureSupport::Supported(path) = fixture
                    .seed_file("async-cleanup-delete-failure", b"payload")
                    .await
                    .expect("seed succeeds")
                else {
                    panic!("memory fixture supports seeded files");
                };
                let mut context = ContractContext::new(fixture.file_system().properties());
                context.record_resource(path, "write/basic");

                context.cleanup_async(fixture.file_system()).await;

                assert_eq!(context.resources.len(), 1, "{fault:?}");
                assert_eq!(context.run.cleanup.failures.len(), 1, "{fault:?}");
                assert!(!fixture.is_empty(), "{fault:?}");
            });
        }
    }

    #[cfg(feature = "async")]
    #[test]
    fn test_async_cleanup_skips_unsupported_delete_and_releases_missing_paths() {
        use qubit_fs::path::Path;

        use crate::AsyncFileSystemFixture;
        use crate::common::AsyncMemoryFixture;
        use crate::common::async_memory_file_system::run_controlled;

        let fixture = AsyncMemoryFixture::without_operation_capabilities();
        run_controlled(async {
            let mut context = ContractContext::new(fixture.file_system().properties());
            context.record_resource(
                Path::parse("/async-cleanup/unsupported").expect("valid path"),
                "write/basic",
            );

            context.cleanup_async(fixture.file_system()).await;

            assert_eq!(context.resources.len(), 1, "unsupported cleanup retains ownership");
            assert!(context.run.cleanup.failures.is_empty());
        });

        let fixture = AsyncMemoryFixture::new();
        run_controlled(async {
            let mut context = ContractContext::new(fixture.file_system().properties());
            context.record_resource(
                Path::parse("/async-cleanup/already-missing").expect("valid path"),
                "write/basic",
            );

            context.cleanup_async(fixture.file_system()).await;

            assert!(context.resources.is_empty(), "missing paths are already cleaned");
            assert!(context.run.cleanup.failures.is_empty());
        });
    }

    #[cfg(feature = "async")]
    #[test]
    fn test_async_cleanup_deletes_recorded_directories() {
        use crate::AsyncFileSystemFixture;
        use crate::common::AsyncMemoryFixture;
        use crate::common::async_memory_file_system::run_controlled;

        let fixture = AsyncMemoryFixture::new();
        run_controlled(async {
            let FixtureSupport::Supported(path) = fixture
                .seed_empty_directory("async-cleanup-recorded-directory")
                .await
                .expect("directory seed succeeds")
            else {
                panic!("memory fixture supports seeded directories");
            };
            let mut context = ContractContext::new(fixture.file_system().properties());
            context.record_resource(path, "directory/create");

            context.cleanup_async(fixture.file_system()).await;

            assert!(context.resources.is_empty());
            assert!(context.run.cleanup.failures.is_empty());
            assert!(fixture.is_empty());
        });
    }
}
