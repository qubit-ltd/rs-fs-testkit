// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Provider paths retained by the contract cleanup ledger.

use qubit_fs::path::Path;

/// One explicitly owned resource created by a contract phase.
pub(crate) struct TrackedResource {
    /// Path the contract runner will inspect and remove during cleanup.
    pub(crate) path: Path,
    /// Contract check that created the path, retained for cleanup diagnostics.
    pub(crate) owner_check: &'static str,
    /// Resource creation order used to keep failed cleanup entries stable.
    pub(crate) creation_index: u64,
}
