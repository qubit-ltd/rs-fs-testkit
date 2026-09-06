// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Structured failures collected while cleaning a contract fixture.

use qubit_fs::path::Path;

use crate::FixtureError;

/// One cleanup operation that failed while later resources were still tried.
pub(crate) struct CleanupFailure {
    pub(crate) owner_check: &'static str,
    pub(crate) operation: &'static str,
    pub(crate) path: Option<Path>,
    pub(crate) cause: FixtureError,
}
