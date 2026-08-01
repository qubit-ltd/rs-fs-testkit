// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use qubit_fs::{CopyMethod, FileSystem, Path};
use qubit_fs_testkit::{FileSystemFixture, FixtureError, FixtureResult, FixtureSupport};

use common::MemoryFixture;

/// Fixture that uses every synchronous optional-hook default.
struct DefaultSyncFixture<'a> {
    file_system: &'a FileSystem,
}

impl FileSystemFixture for DefaultSyncFixture<'_> {
    fn file_system(&self) -> &FileSystem {
        self.file_system
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/defaults/{relative}"))
            .map_err(|error| FixtureError::new(error.to_string()))
    }
}

/// Optional synchronous fixture hooks report unavailable until a provider opts
/// in.
#[test]
fn test_file_system_fixture_defaults_are_unsupported() {
    let memory = MemoryFixture::new();
    let fixture = DefaultSyncFixture {
        file_system: memory.file_system(),
    };
    let path = fixture.path("entry").expect("build default path");
    assert_eq!(
        fixture
            .list_prefix(&Path::root(), "entry")
            .expect("build default list prefix"),
        "entry"
    );
    assert!(matches!(
        fixture.seed_file("entry", b"bytes"),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.read_file(&path),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.resource_version(&path),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.seed_empty_directory("directory"),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.seed_symlink("link"),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.copy_fast_path_case(CopyMethod::Native),
        Ok(FixtureSupport::Unsupported)
    ));
}
