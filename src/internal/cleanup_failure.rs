// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Structured failures collected while cleaning a contract fixture.

use qubit_fs::path::Path;

use crate::FixtureError;

/// One cleanup operation that failed while later resources were still tried.
pub(crate) struct CleanupFailure {
    /// Check that created or owns the resource whose cleanup failed.
    pub(crate) owner_check: &'static str,
    /// Cleanup operation that failed, such as `stat` or `delete`.
    pub(crate) operation: &'static str,
    /// Resource path involved in the failure, when it is known.
    pub(crate) path: Option<Path>,
    /// Failure details, including any underlying filesystem error.
    pub(crate) cause: FixtureError,
}
