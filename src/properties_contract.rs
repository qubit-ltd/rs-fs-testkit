// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Contract assertions for filesystem identity, limits, and capabilities.

use std::ptr;

use qubit_fs::{
    FileSystemCapabilities,
    FileSystemCapability,
    FsOperation,
};

use crate::FileSystemFixture;

const CAPABILITY_DEPENDENCIES: &[(
    FileSystemCapability,
    FileSystemCapability,
    &str,
)] = &[
    (
        FileSystemCapability::RangeRead,
        FileSystemCapability::Read,
        "RangeRead requires Read",
    ),
    (
        FileSystemCapability::ConditionalRead,
        FileSystemCapability::Read,
        "ConditionalRead requires Read",
    ),
    (
        FileSystemCapability::ChecksumValidation,
        FileSystemCapability::Read,
        "ChecksumValidation requires Read",
    ),
    (
        FileSystemCapability::Append,
        FileSystemCapability::Write,
        "Append requires Write",
    ),
    (
        FileSystemCapability::ConditionalWrite,
        FileSystemCapability::Write,
        "ConditionalWrite requires Write",
    ),
    (
        FileSystemCapability::AtomicReplace,
        FileSystemCapability::Write,
        "AtomicReplace requires Write",
    ),
    (
        FileSystemCapability::RecursiveDelete,
        FileSystemCapability::Delete,
        "RecursiveDelete requires Delete",
    ),
    (
        FileSystemCapability::ConditionalDelete,
        FileSystemCapability::Delete,
        "ConditionalDelete requires Delete",
    ),
    (
        FileSystemCapability::AtomicRename,
        FileSystemCapability::Rename,
        "AtomicRename requires Rename",
    ),
    (
        FileSystemCapability::ServerSideCopy,
        FileSystemCapability::Copy,
        "ServerSideCopy requires Copy",
    ),
];

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
    for &(derived, required, message) in CAPABILITY_DEPENDENCIES {
        assert_capability_dependency(capabilities, derived, required, message);
    }
}

/// Checks one capability dependency.
///
/// # Parameters
///
/// * `capabilities` - Advertised capability set.
/// * `derived` - Capability whose presence creates the dependency.
/// * `required` - Capability required by `derived`.
/// * `message` - Contract failure message.
///
/// # Panics
///
/// Panics when `derived` is present and `required` is absent.
fn assert_capability_dependency(
    capabilities: FileSystemCapabilities,
    derived: FileSystemCapability,
    required: FileSystemCapability,
    message: &str,
) {
    assert!(
        !capabilities.contains(derived) || capabilities.contains(required),
        "{message}",
    );
}
