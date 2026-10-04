// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements fixture adaptation and suite lifecycle support.

use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::panic::resume_unwind;

use super::FileSystemCapability;
use super::FileSystemContractSuite;
use super::Path;
use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::FixtureError;
use crate::FixtureSupport;

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
        self.context.run.cleanup.attempts += 1;
        self.context.run.cleanup.completed = false;
        let failure_start = self.context.run.cleanup.failures.len();
        self.context.cleanup(self.fixture.file_system());
        let mut teardown_failure = None;
        if !self.teardown_completed {
            let teardown = catch_unwind(AssertUnwindSafe(|| self.fixture.teardown()));
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
    pub(super) fn check_error_context(&mut self) -> Result<(), crate::ContractFailure> {
        let id = ContractCheckId::ErrorContext;
        self.context.begin("error_context");
        let relative = self.context.relative_name("missing");
        let path = self.fixture.path(&relative).map_err(|error| {
            crate::ContractFailure::with_source("error/context: path preparation failed", error).at(id)
        })?;
        let error = match self.fixture.file_system().stat(&path) {
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
        self.fixture.path(&relative).expect("contract: fixture path failed")
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
    #[inline]
    pub fn capable(&self, capability: FileSystemCapability) -> bool {
        self.context.properties().capabilities().supports(capability)
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
            FixtureSupport::Unsupported => {
                panic!("{contract} contract: advertised capability requires fixture.seed_file support")
            }
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
        let support = self
            .fixture
            .seed_file(&relative, bytes)
            .expect("contract: fixture seed failed");
        if let FixtureSupport::Supported(path) = &support {
            self.context.record_created(path.clone());
        }
        support
    }
}

#[cfg(test)]
mod tests {
    use qubit_fs::FileSystem;
    use qubit_fs::path::Path;

    use crate::FileSystemContract;
    use crate::FileSystemContractSuite;
    use crate::FileSystemFixture;
    use crate::FixtureError;
    use crate::FixtureResult;
    use crate::common::MemoryFixture;

    struct TeardownFixture {
        inner: MemoryFixture,
        panic: bool,
    }

    impl FileSystemFixture for TeardownFixture {
        fn file_system(&self) -> &FileSystem {
            self.inner.file_system()
        }

        fn path(&self, relative: &str) -> FixtureResult<Path> {
            self.inner.path(relative)
        }

        fn teardown(&self) -> FixtureResult<()> {
            if self.panic {
                panic!("injected fixture teardown panic");
            }
            Err(FixtureError::new("injected fixture teardown failure"))
        }
    }

    #[test]
    fn finish_capture_retains_teardown_error_and_panic() {
        for panic in [false, true] {
            let fixture = TeardownFixture {
                inner: MemoryFixture::new(),
                panic,
            };
            let mut suite = FileSystemContractSuite::new(&fixture);
            let run = suite.run_contract(FileSystemContract::ErrorContext);
            assert!(!run.cleanup().failures().is_empty());
            assert!(!run.requirements_satisfied());
        }
    }
}
