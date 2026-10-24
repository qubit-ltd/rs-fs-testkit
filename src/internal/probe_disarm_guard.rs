// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cancelling a probe releases its gate only after its execution is dropped.

use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;

use crate::ContractFailure;
use crate::FixtureResult;

/// Releases a fixture gate once and retains errors even during unwinding.
///
/// Declare this guard before the execution future so reverse drop order
/// cancels provider execution first. Disarming never starts filesystem I/O.
pub(crate) struct ProbeDisarmGuard<'a> {
    /// Operation that releases the fixture's probe gate.
    disarm: Box<dyn Fn() -> FixtureResult<()> + Send + Sync + 'a>,
    /// Suite-owned sink that retains disarm failures.
    failures: &'a mut Vec<ContractFailure>,
    /// Whether the release operation still needs to be attempted.
    armed: bool,
}

impl<'a> ProbeDisarmGuard<'a> {
    /// Creates a guard that releases a probe gate and records release failures.
    ///
    /// # Parameters
    ///
    /// - `disarm`: operation that releases the gate without starting filesystem
    ///   I/O.
    /// - `failures`: suite-owned vector that receives any release error or
    ///   panic.
    ///
    /// # Returns
    ///
    /// A guard armed to invoke `disarm` once, either explicitly or during drop.
    pub(crate) fn new(
        disarm: impl Fn() -> FixtureResult<()> + Send + Sync + 'a,
        failures: &'a mut Vec<ContractFailure>,
    ) -> Self {
        Self {
            disarm: Box::new(disarm),
            failures,
            armed: true,
        }
    }

    /// Disarms once, recording the original error or panic without unwinding.
    ///
    /// # Returns
    ///
    /// `true` if cleanup succeeded or had already been attempted; `false` if
    /// cleanup returned an error or panicked.
    pub(crate) fn disarm(&mut self) -> bool {
        if !self.armed {
            return true;
        }
        self.armed = false;
        match catch_unwind(AssertUnwindSafe(|| (self.disarm)())) {
            Ok(Ok(())) => true,
            Ok(Err(error)) => {
                self.failures
                    .push(ContractFailure::with_source("probe disarm failed", error));
                false
            }
            Err(payload) => {
                self.failures
                    .push(ContractFailure::panicked("probe disarm panicked", payload));
                false
            }
        }
    }
}

impl Drop for ProbeDisarmGuard<'_> {
    /// Records disarm failures without causing a second unwind.
    fn drop(&mut self) {
        let _ = self.disarm();
    }
}
