// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::metadata::ResourceVersion;

use crate::ReadScenario;
use crate::internal::read_expectations::bytes;
use crate::internal::read_expectations::options;

#[test]
fn test_small_ranges_use_interior_offset_and_independent_bytes() {
    for (maximum, expected) in [
        (1, b"c".as_slice()),
        (4, b"cont".as_slice()),
        (7, b"contrac".as_slice()),
        (8, b"contract".as_slice()),
    ] {
        let limit = FileSystemLimit::Maximum(maximum);
        let request = options(ReadScenario::Range, limit, ResourceVersion::new("unused"));
        assert_eq!(request.offset(), Some(5));
        assert_eq!(request.length(), Some(maximum));
        assert_eq!(bytes(ReadScenario::Range, limit), expected);
    }
}

#[test]
fn test_non_maximum_ranges_use_default_interior_window() {
    for limit in [
        FileSystemLimit::Unknown,
        FileSystemLimit::Unbounded,
        FileSystemLimit::NotApplicable,
    ] {
        let request = options(ReadScenario::Range, limit, ResourceVersion::new("unused"));
        assert_eq!(request.offset(), Some(5));
        assert_eq!(request.length(), Some(8));
        assert_eq!(bytes(ReadScenario::Range, limit), b"contract");
    }
}

#[test]
fn test_zero_maximum_range_has_zero_offset_length_and_bytes() {
    let limit = FileSystemLimit::Maximum(0);
    let request = options(ReadScenario::Range, limit, ResourceVersion::new("unused"));
    assert_eq!(request.offset(), Some(0));
    assert_eq!(request.length(), Some(0));
    assert!(bytes(ReadScenario::Range, limit).is_empty());
}
