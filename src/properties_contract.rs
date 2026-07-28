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
    FileSystemExt,
    FileSystemLimit,
    FsErrorKind,
    FsOperation,
};

use crate::{
    FileSystemFixture,
    internal::assert_error,
};

const MAX_LIMIT_PROBE_BYTES: usize = 4096;

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
#[track_caller]
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

    assert_finite_path_limit(
        fixture,
        file_system.limits().max_path_text_bytes(),
        "contract-limit-path-",
        "path limits must reject oversized paths",
    );
    if file_system.info().path_semantics()
        == qubit_fs::PathSemantics::Hierarchical
    {
        assert_finite_path_limit(
            fixture,
            file_system.limits().max_component_text_bytes(),
            "",
            "component limits must reject oversized paths",
        );
    }
    if let Some(length) =
        oversized_probe_length(file_system.limits().max_write_bytes())
    {
        let path = fixture.path("contract-limit-write.bin");
        let bytes = vec![b'x'; length];
        let error = file_system
            .write_all(&path, &bytes)
            .expect_err("write limits must reject oversized payloads");
        assert_error(
            &error,
            FsErrorKind::ResourceLimitExceeded,
            FsOperation::Write,
            Some(&path),
            None,
            None,
        );
    }
}

/// Checks that a finite path-related limit rejects a safely sized probe.
///
/// # Parameters
///
/// * `fixture` - Provider fixture that maps the generated relative path.
/// * `limit` - Declared path or component limit.
/// * `prefix` - Text prepended to the generated oversized component.
/// * `message` - Assertion message describing the asserted limit.
///
/// # Panics
///
/// Panics when a declared finite limit accepts the oversized path or returns an
/// incorrectly structured error.
#[track_caller]
fn assert_finite_path_limit(
    fixture: &dyn FileSystemFixture,
    limit: FileSystemLimit,
    prefix: &str,
    message: &str,
) {
    let Some(length) = oversized_probe_length(limit) else {
        return;
    };
    let relative = format!("{prefix}{}", "x".repeat(length));
    let path = fixture.path(&relative);
    let file_system = fixture.file_system();
    let error = file_system.stat(&path).expect_err(message);
    assert_error(
        &error,
        FsErrorKind::ResourceLimitExceeded,
        FsOperation::Stat,
        Some(&path),
        Some(file_system.info().provider_id()),
        None,
    );
}

/// Calculates an oversized probe length without allocating unbounded memory.
///
/// # Parameters
///
/// * `limit` - Declared filesystem limit.
///
/// # Returns
/// A length one byte above a finite, safely probeable maximum; `None` for
/// unknown, unbounded, inapplicable, or excessively large limits.
#[must_use]
fn oversized_probe_length(limit: FileSystemLimit) -> Option<usize> {
    let FileSystemLimit::Maximum(maximum) = limit else {
        return None;
    };
    let maximum = usize::try_from(maximum).ok()?;
    maximum
        .checked_add(1)
        .filter(|length| *length <= MAX_LIMIT_PROBE_BYTES)
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
#[track_caller]
pub fn assert_capabilities_contract(fixture: &dyn FileSystemFixture) {
    let capabilities = fixture.file_system().capabilities();
    if let Some((derived, required)) = capabilities.missing_dependency() {
        panic!("{derived:?} requires {required:?}");
    }
}
