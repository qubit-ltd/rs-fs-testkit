// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Tests for the crate-internal construction and public inspection of retained
//! errors.

use crate::ContractSource;

#[test]
fn test_source_inspection_and_transfer_preserve_original_error() {
    let source = ContractSource::new(std::io::Error::other("source"));
    assert_eq!(source.inspect(|error| error.to_string()), Some("source".to_owned()));
    assert!(format!("{source:?}").contains("retained"));
    assert!(format!("{source}").contains("inspect"));
    assert_eq!(source.take().expect("source must transfer").to_string(), "source");
    assert!(source.inspect(|_| ()).is_none());
    assert!(source.take().is_none());
}

#[test]
fn test_formatting_keeps_the_original_error_available_for_inspection() {
    let source = ContractSource::new(std::io::Error::other("private provider detail"));
    assert_eq!(
        format!("{source}"),
        "original provider failure; inspect the retained source for recovery"
    );
    assert!(format!("{source:?}").contains("original_error: retained"));
    assert_eq!(
        source.inspect(|error| error.to_string()),
        Some("private provider detail".to_owned())
    );
    assert_eq!(
        source.take().expect("source must remain available").to_string(),
        "private provider detail"
    );
    assert!(source.inspect(|error| error.to_string()).is_none());
    assert!(source.inspect(|error| error.to_string().len()).is_none());
}
