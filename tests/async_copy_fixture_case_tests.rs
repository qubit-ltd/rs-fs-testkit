// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_fs::{
    CopyOptions,
    Path,
};
use qubit_fs_testkit::AsyncCopyFixtureCase;

/// Prepared asynchronous copy cases preserve and transfer their request parts.
#[test]
fn test_async_copy_fixture_case_exposes_and_transfers_request_parts() {
    let source = Path::parse("/defaults/source").expect("valid source path");
    let target = Path::parse("/defaults/target").expect("valid target path");
    let case = AsyncCopyFixtureCase::new(
        source.clone(),
        target.clone(),
        CopyOptions::default(),
    );

    assert_eq!(case.source(), &source);
    assert_eq!(case.target(), &target);
    assert_eq!(case.options(), &CopyOptions::default());

    let (actual_source, actual_target, actual_options) = case.into_parts();
    assert_eq!(actual_source, source);
    assert_eq!(actual_target, target);
    assert_eq!(actual_options, CopyOptions::default());
}
