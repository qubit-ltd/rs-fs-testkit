// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements fixture adaptation and suite lifecycle support.

use super::*;
use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::panic::resume_unwind;
use crate::ContractCheckOutcome;
use crate::FixtureError;

impl<'a> FileSystemContractSuite<'a> {
    /// Cleans resources created by individually executed contract phases.
    ///
    /// # Panics
    ///
    /// Panics when a recorded resource cannot be inspected or deleted.
    pub fn finish(&mut self) {
        if let Err(payload) = self.finish_capture() {
            resume_unwind(payload);
        }
    }

    /// Runs facade cleanup and fixture teardown while preserving failures.
    pub(super) fn finish_capture(&mut self) -> Result<(), Box<dyn Any + Send>> {
        let failures = self.context.cleanup(self.fixture.file_system());
        let mut teardown_failure = None;
        if !self.teardown_completed {
            let teardown = catch_unwind(AssertUnwindSafe(|| self.fixture.teardown()));
            match teardown {
                Ok(Ok(support)) => {
                    self.teardown_completed = true;
                    if matches!(support, FixtureSupport::Unsupported)
                        && self.context.resources_prepared()
                    {
                        self.context.record_check(
                            "cleanup/fixture-teardown",
                            None,
                            ContractCheckOutcome::Unverified {
                                reason: "fixture teardown is unavailable".to_owned(),
                            },
                        );
                    }
                }
                Ok(Err(error)) => {
                    teardown_failure = Some(format!("[fs-testkit:cleanup/teardown] {error}"));
                }
                Err(_payload) => {
                    teardown_failure = Some(
                        "[fs-testkit:cleanup/teardown] fixture teardown panicked".to_owned(),
                    );
                }
            }
        }
        if failures.is_empty() && teardown_failure.is_none() {
            return Ok(());
        }
        let mut summary = failures
            .iter()
            .map(|failure| {
                format!(
                    "[fs-testkit:cleanup/{}] owner={} path={:?}: {}",
                    failure.operation,
                    failure.owner_check,
                    failure.path,
                    failure.cause,
                )
            })
            .collect::<Vec<_>>();
        if let Some(failure) = teardown_failure {
            summary.push(failure);
        }
        Err(Box::new(FixtureError::new(summary.join("; "))))
    }

    /// Checks structured filesystem error context and redaction behavior.
    ///
    /// # Panics
    ///
    /// Panics when a missing-path error omits or misreports its structured
    /// kind, operation, or path context.
    pub fn assert_error_context(&mut self) {
        self.context.begin("error_context");
        let path = self.path("error-context-missing");
        let error = self
            .fixture
            .file_system()
            .stat(&path)
            .expect_err("error/context: missing path succeeded");
        self.assert_error(
            &error,
            FsErrorKind::NotFound,
            FsOperation::Stat,
            &path,
            None,
        );
    }

    /// Resolves a fixture path or identifies the contract that could not set
    /// up.
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
            .expect("contract: fixture path failed")
    }

    /// Returns whether the immutable snapshot declares a capability.
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
        self.context
            .properties()
            .capabilities()
            .supports(capability)
    }

    /// Seeds a resource and makes support mandatory for the requested
    /// capability.
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
    #[inline]
    pub fn required_seed(&mut self, relative: &str, bytes: &[u8], contract: &str) -> Path {
        match self.seed(relative, bytes) {
            FixtureSupport::Supported(path) => path,
            FixtureSupport::Unsupported => panic!(
                "{contract} contract: advertised capability requires fixture.seed_file support"
            ),
        }
    }

    /// Delegates out-of-band resource preparation to the fixture.
    ///
    /// # Parameters
    ///
    /// * `relative` - Resource name relative to the current contract phase.
    /// * `bytes` - Exact content to publish.
    ///
    /// # Returns
    ///
    /// The fixture's support result and seeded path, when available.
    ///
    /// # Panics
    ///
    /// Panics when provider-specific fixture setup returns an error.
    #[inline]
    pub fn seed(&mut self, relative: &str, bytes: &[u8]) -> FixtureSupport<Path> {
        let relative = self.context.relative_name(relative);
        let support = self.fixture
            .seed_file(&relative, bytes)
            .expect("contract: fixture seed failed");
        if let FixtureSupport::Supported(path) = &support {
            self.context.record_created(path.clone());
        }
        support
    }

    /// Reads a provider-owned probe through the fixture and checks exact bytes.
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
    pub fn assert_bytes(&self, path: &Path, expected: &[u8], message: &str) {
        match self
            .fixture
            .read_file(path)
            .expect("copy contract: fixture observation failed")
        {
            FixtureSupport::Supported(actual) => {
                assert_eq!(actual, expected, "{message}")
            }
            FixtureSupport::Unsupported => {
                panic!("{message}: Copy capability requires fixture.read_file support")
            }
        }
    }

    /// Validates public error context without exposing provider implementation
    /// details.
    ///
    /// # Parameters
    ///
    /// * `error` - Actual filesystem error.
    /// * `kind` - Expected error classification.
    /// * `operation` - Expected public operation.
    /// * `path` - Expected source path.
    /// * `target` - Expected destination path, when applicable.
    ///
    /// # Panics
    ///
    /// Panics when any expected structured field differs.
    pub fn assert_error(
        &self,
        error: &FsError,
        kind: FsErrorKind,
        operation: FsOperation,
        path: &Path,
        target: Option<&Path>,
    ) {
        let provider = Some(self.context.properties().info().provider_id());
        if kind == FsErrorKind::UnsupportedCapability && target.is_none() {
            assert_unsupported_error(
                error,
                kind,
                operation,
                Some(path),
                provider,
                error.required_capability(),
            );
        } else if kind == FsErrorKind::AlreadyExists
            && let Some(target) = target
        {
            assert_error_with_source_or_target(
                error,
                kind,
                operation,
                path,
                target,
                provider,
                error.required_capability(),
            );
        } else {
            assert_error_with_target(
                error,
                kind,
                operation,
                Some(path),
                target,
                provider,
                error.required_capability(),
            );
        }
    }

    /// Validates an operation error that has no logical input path.
    pub fn assert_pathless_error(
        &self,
        error: &FsError,
        kind: FsErrorKind,
        operation: FsOperation,
    ) {
        assert_unsupported_error(
            error,
            kind,
            operation,
            None,
            Some(self.context.properties().info().provider_id()),
            error.required_capability(),
        );
    }

    /// Validates option-derived capability preflight errors.
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
    pub fn assert_requirement_error(
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
