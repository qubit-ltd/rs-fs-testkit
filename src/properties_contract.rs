// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Contract assertions for filesystem identity, limits, and capabilities.

use std::ptr;

use qubit_fs::FsOperation;

use crate::FileSystemFixture;

/// Checks stable construction-time filesystem properties.
///
/// # Parameters
///
/// * `fixture` - Isolated provider fixture whose properties are checked.
///
/// # Panics
///
/// Panics when identity fields are empty, property snapshots are unstable, or
/// the fixture returns a path that violates its declared semantics or limits.
pub fn assert_properties_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    let info = file_system.info();
    assert!(
        !info.provider_id().is_empty(),
        "provider identifiers must not be empty",
    );
    assert!(
        !info.id().as_str().is_empty(),
        "filesystem identifiers must not be empty",
    );
    assert!(
        ptr::eq(info, file_system.info()),
        "filesystem info must be a stable construction-time snapshot",
    );
    assert!(
        ptr::eq(file_system.limits(), file_system.limits()),
        "filesystem limits must be a stable construction-time snapshot",
    );
    assert_eq!(
        file_system.capabilities(),
        file_system.capabilities(),
        "filesystem capabilities must be stable",
    );

    let path = fixture.path("contract-properties.bin");
    file_system
        .limits()
        .validate_path(
            &path,
            file_system.info().path_semantics(),
            FsOperation::Stat,
        )
        .expect("fixture paths must satisfy declared filesystem limits");
}

/// Checks dependency relationships between advertised capabilities.
///
/// # Parameters
///
/// * `fixture` - Provider fixture whose advertised capabilities are checked.
///
/// # Panics
///
/// Panics when a derived capability is advertised without its required base
/// capability.
pub fn assert_capabilities_contract(fixture: &dyn FileSystemFixture) {
    let capabilities = fixture.file_system().capabilities();
    if let Some((derived, required)) = capabilities.missing_dependency() {
        panic!("{derived:?} requires {required:?}");
    }
}
