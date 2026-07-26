// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Reusable contract assertions for filesystem provider implementations.

#![deny(missing_docs)]

use qubit_fs::FileSystemProperties;

/// Checks construction-time filesystem identity invariants.
///
/// # Panics
/// Panics when a provider exposes an empty provider identifier or filesystem
/// identifier.
pub fn assert_properties_contract(properties: &dyn FileSystemProperties) {
    let info = properties.info();
    assert!(
        !info.provider_id().is_empty(),
        "provider identifiers must not be empty",
    );
    assert!(
        !info.id().as_str().is_empty(),
        "filesystem identifiers must not be empty",
    );
}
