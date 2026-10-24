// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Tests for retaining probe disarm failures.

use crate::FixtureError;
use crate::internal::probe_disarm_guard::ProbeDisarmGuard;

#[test]
fn test_disarm_records_an_error_once() {
    let mut failures = Vec::new();
    let mut guard = ProbeDisarmGuard::new(|| Err(FixtureError::new("disarm failure")), &mut failures);
    assert!(!guard.disarm());
    assert!(guard.disarm());
    drop(guard);
    assert_eq!(failures.len(), 1);
    assert!(failures[0].message().contains("probe disarm failed"));
}

#[test]
fn test_drop_retains_a_disarm_panic() {
    let mut failures = Vec::new();
    let guard = ProbeDisarmGuard::new(|| -> crate::FixtureResult<()> { panic!("disarm panic") }, &mut failures);
    drop(guard);
    assert_eq!(failures.len(), 1);
    assert!(failures[0].message().contains("probe disarm panicked: disarm panic"));
    assert!(failures[0].take_panic_payload().is_some());
}
