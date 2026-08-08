// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_fs_testkit::FixtureError;

/// Fixture errors retain messages and sources while redacting source debug
/// text.
#[test]
fn test_fixture_error_preserves_message_and_source() {
    let plain = FixtureError::new("plain failure");
    assert_eq!(plain.to_string(), "plain failure");
    assert!(std::error::Error::source(&plain).is_none());

    let sourced = FixtureError::with_source(
        "outer failure",
        std::io::Error::other("inner"),
    );
    assert_eq!(sourced.to_string(), "outer failure");
    assert_eq!(
        std::error::Error::source(&sourced)
            .expect("preserved source")
            .to_string(),
        "inner"
    );
    let debug = format!("{sourced:?}");
    assert!(debug.contains("outer failure"));
    assert!(!debug.contains("inner"));
}
