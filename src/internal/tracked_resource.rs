// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Provider paths retained by the contract cleanup ledger.

use qubit_fs::path::Path;

/// One explicitly owned resource created by a contract phase.
pub(crate) struct TrackedResource {
    pub(crate) path: Path,
    pub(crate) owner_check: &'static str,
    pub(crate) creation_index: u64,
}
