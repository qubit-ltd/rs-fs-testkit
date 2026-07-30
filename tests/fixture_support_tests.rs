// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_fs::Path;
use qubit_fs_testkit::{
    FixtureError,
    FixtureResult,
    FixtureSupport,
};

/// Verifies that setup failures remain distinguishable from unavailable probes.
#[test]
fn test_fixture_support_does_not_conflate_error_with_unsupported() {
    let unsupported: FixtureResult<FixtureSupport<Path>> =
        Ok(FixtureSupport::Unsupported);
    let failure: FixtureResult<FixtureSupport<Path>> =
        Err(FixtureError::new("setup failed"));

    assert!(matches!(unsupported, Ok(FixtureSupport::Unsupported)));
    assert!(failure.is_err());
}
