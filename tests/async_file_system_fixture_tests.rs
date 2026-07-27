// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_fs::{AsyncFileSystem, FsPath};
use qubit_fs_testkit::AsyncFileSystemFixture;

struct FixtureTypeCheck;

impl AsyncFileSystemFixture for FixtureTypeCheck {
    fn file_system(&self) -> &dyn AsyncFileSystem {
        panic!("type-check fixture has no runtime filesystem")
    }

    fn path(&self, relative: &str) -> FsPath {
        FsPath::parse(relative).expect("fixture path should parse")
    }
}

#[test]
fn test_async_fixture_is_object_safe() {
    let fixture = FixtureTypeCheck;
    let fixture: &dyn AsyncFileSystemFixture = &fixture;

    assert_eq!(
        FsPath::parse("contract").expect("expected path should parse"),
        fixture.path("contract"),
    );
}
