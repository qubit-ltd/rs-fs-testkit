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
    disarm: Box<dyn Fn() -> FixtureResult<()> + Send + Sync + 'a>,
    failures: &'a mut Vec<ContractFailure>,
    armed: bool,
}

impl<'a> ProbeDisarmGuard<'a> {
    /// Creates a guard with a suite-owned diagnostic sink.
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

#[cfg(test)]
mod tests {
    use super::ProbeDisarmGuard;
    use crate::FixtureError;

    #[test]
    fn disarm_records_an_error_once() {
        let mut failures = Vec::new();
        let mut guard = ProbeDisarmGuard::new(|| Err(FixtureError::new("disarm failure")), &mut failures);
        assert!(!guard.disarm());
        assert!(guard.disarm());
        drop(guard);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].message().contains("probe disarm failed"));
    }

    #[test]
    fn drop_retains_a_disarm_panic() {
        let mut failures = Vec::new();
        let guard = ProbeDisarmGuard::new(|| -> crate::FixtureResult<()> { panic!("disarm panic") }, &mut failures);
        drop(guard);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].message().contains("probe disarm panicked: disarm panic"));
        assert!(failures[0].take_panic_payload().is_some());
    }
}
