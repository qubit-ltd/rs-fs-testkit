// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements fixture adaptation and suite lifecycle support.

use std::any::Any;
use std::panic::resume_unwind;

use super::*;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::FixtureError;

impl<'a> AsyncFileSystemContractSuite<'a> {
    /// Cleans resources created by individually executed asynchronous phases.
    ///
    /// # Panics
    ///
    /// Panics when a recorded resource cannot be inspected or deleted.
    pub async fn finish(&mut self) {
        if let Err(payload) = self.finish_capture().await {
            resume_unwind(payload);
        }
    }

    /// Runs asynchronous cleanup and fixture teardown while preserving
    /// failures.
    pub(super) async fn finish_capture(&mut self) -> Result<(), Box<dyn Any + Send>> {
        self.context.run.cleanup.attempts += 1;
        self.context.run.cleanup.completed = false;
        let failure_start = self.context.run.cleanup.failures.len();
        self.context.cleanup_async(self.fixture.file_system()).await;
        let mut teardown_failure = None;
        if !self.teardown_completed {
            let teardown = crate::internal::catch_unwind_future(self.fixture.teardown()).await;
            match teardown {
                Ok(Ok(())) => {
                    self.teardown_completed = true;
                }
                Ok(Err(error)) => {
                    teardown_failure = Some(format!("[fs-testkit:cleanup/teardown] {error}"));
                    self.context
                        .run
                        .cleanup
                        .failures
                        .push(crate::ContractFailure::with_source("fixture teardown failed", error));
                }
                Err(payload) => {
                    self.context
                        .run
                        .cleanup
                        .failures
                        .push(crate::ContractFailure::panicked("fixture teardown panicked", payload));
                    teardown_failure = Some("[fs-testkit:cleanup/teardown] fixture teardown panicked".to_owned());
                }
            }
        }
        if self.context.run.cleanup.failures.len() == failure_start && teardown_failure.is_none() {
            self.context.run.cleanup.completed = true;
            return Ok(());
        }
        let mut summary = self.context.run.cleanup.failures[failure_start..]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if let Some(failure) = teardown_failure {
            summary.push(failure);
        }
        Err(Box::new(FixtureError::new(summary.join("; "))))
    }

    /// Returns an ordinary structured failure for fixture or contract errors.
    pub(super) async fn check_error_context(&mut self) -> Result<(), crate::ContractFailure> {
        let id = ContractCheckId::ErrorContext;
        self.context.begin("error_context");
        let relative = self.context.relative_name("missing");
        let path = self.fixture.path(&relative).map_err(|error| {
            crate::ContractFailure::with_source("error/context: path preparation failed", error).at(id)
        })?;
        let error = match self.fixture.file_system().stat(&path).await {
            Ok(_) => {
                return Err(
                    crate::ContractFailure::message_only("error/context: missing path unexpectedly exists").at(id),
                );
            }
            Err(error) => error,
        };
        crate::internal::verify_missing_error(error, &path, self.context.properties().info().provider_id(), id)?;
        self.context.record_check(id, None, ContractCheckOutcome::Passed);
        Ok(())
    }

    /// Resolves a fixture path or identifies setup failure at the contract
    /// boundary.
    ///
    /// # Parameters
    ///
    /// * `relative` - Resource name relative to the current contract phase.
    ///
    /// # Returns
    ///
    /// The provider path mapped by the fixture.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot map the generated relative name.
    #[inline]
    pub fn path(&self, relative: &str) -> Path {
        let relative = self.context.relative_name(relative);
        self.fixture
            .path(&relative)
            .expect("async contract: fixture path failed")
    }

    /// Returns whether the cached property snapshot advertises a capability.
    ///
    /// # Parameters
    ///
    /// * `capability` - Capability to query in the captured snapshot.
    ///
    /// # Returns
    ///
    /// `true` when the provider advertises the capability.
    #[inline(always)]
    pub fn capable(&self, capability: FileSystemCapability) -> bool {
        self.context.properties().capabilities().supports(capability)
    }

    /// Seeds a resource and makes fixture support mandatory for the contract.
    ///
    /// # Parameters
    ///
    /// * `relative` - Resource name relative to the current contract phase.
    /// * `bytes` - Exact content to publish.
    /// * `contract` - Contract label used in failure diagnostics.
    ///
    /// # Returns
    ///
    /// The provider path of the seeded file.
    ///
    /// # Panics
    ///
    /// Panics when fixture setup fails or the required seed hook is
    /// unsupported.
    pub async fn required_seed(&mut self, relative: &str, bytes: &[u8], contract: &str) -> Path {
        let relative = self.context.relative_name(relative);
        match self
            .fixture
            .seed_file(&relative, bytes)
            .await
            .expect("async contract: fixture seed failed")
        {
            FixtureSupport::Supported(path) => {
                self.context.record_created(path.clone());
                path
            }
            FixtureSupport::Unsupported => {
                panic!("{contract} contract: advertised capability requires fixture.seed_file support")
            }
        }
    }

    /// Reads a fixture-owned file and checks its exact bytes after copy.
    ///
    /// # Parameters
    ///
    /// * `path` - Provider path to observe.
    /// * `expected` - Exact expected content.
    /// * `message` - Assertion message used when content differs.
    ///
    /// # Panics
    ///
    /// Panics when observation fails, is unsupported, or returns different
    /// content.
    #[allow(dead_code)]
    pub(super) async fn assert_bytes(&self, path: &Path, expected: &[u8], message: &str) {
        match self
            .fixture
            .read_file(path)
            .await
            .expect("async copy contract: fixture observation failed")
        {
            FixtureSupport::Supported(actual) => {
                assert_eq!(actual, expected, "{message}")
            }
            FixtureSupport::Unsupported => {
                panic!("{message}: Copy capability requires fixture.read_file support")
            }
        }
    }

    /// Checks public context on an asynchronous facade error.
    ///
    /// # Parameters
    ///
    /// * `error` - Actual filesystem error.
    /// * `kind` - Expected error classification.
    /// * `operation` - Expected public operation.
    /// * `path` - Expected source path.
    ///
    /// # Panics
    ///
    /// Panics when any expected structured field differs.
    #[allow(dead_code)]
    pub(super) fn assert_error(&self, error: &FsError, kind: FsErrorKind, operation: FsOperation, path: &Path) {
        let provider = Some(self.context.properties().info().provider_id());
        if kind == FsErrorKind::UnsupportedCapability {
            assert_unsupported_error(
                error,
                kind,
                operation,
                Some(path),
                provider,
                error.required_capability(),
            );
        } else {
            assert_error_with_target(
                error,
                kind,
                operation,
                Some(path),
                None,
                provider,
                error.required_capability(),
            );
        }
    }

    /// Validates an asynchronous operation error with no logical input path.
    #[allow(dead_code)]
    pub(super) fn assert_pathless_error(&self, error: &FsError, kind: FsErrorKind, operation: FsOperation) {
        assert_unsupported_error(
            error,
            kind,
            operation,
            None,
            Some(self.context.properties().info().provider_id()),
            error.required_capability(),
        );
    }

    /// Validates option-derived asynchronous capability preflight errors.
    ///
    /// # Parameters
    ///
    /// * `error` - Actual filesystem error.
    /// * `operation` - Expected public operation.
    /// * `capability` - Capability required by the rejected options.
    /// * `contract` - Contract label used in diagnostics.
    ///
    /// # Panics
    ///
    /// Panics when the error kind, operation, or required capability differs.
    #[allow(dead_code)]
    pub(super) fn assert_requirement_error(
        &self,
        error: &FsError,
        operation: FsOperation,
        capability: FileSystemCapability,
        contract: &str,
    ) {
        let _ = contract;
        assert_unsupported_error(
            error,
            FsErrorKind::RequirementNotMet,
            operation,
            error.path(),
            Some(self.context.properties().info().provider_id()),
            Some(capability),
        );
    }
}
