// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

pub use ::qubit_fs_testkit;
mod common;
use qubit_fs::FileSystem;
use qubit_fs::copy::CopyMethod;
use qubit_fs::copy::CopyOptions;
use qubit_fs::path::Path;
use qubit_fs_testkit::CopyFixtureCase;
use qubit_fs_testkit::CopyScenario;
use qubit_fs_testkit::DeleteScenario;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixturePreparation;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;
use qubit_fs_testkit::ReadScenario;
use qubit_fs_testkit::WriteScenario;

use self::common::MemoryFixture;
/// Fixture that uses every synchronous optional-hook default.
struct DefaultSyncFixture<'a> {
    file_system: &'a FileSystem,
}

/// Fixture implementing only the independent setup hooks used by defaults.
struct PreparedSyncFixture<'a> {
    file_system: &'a FileSystem,
}

impl FileSystemFixture for PreparedSyncFixture<'_> {
    fn teardown(&self) -> FixtureResult<()> {
        Ok(())
    }

    fn file_system(&self) -> &FileSystem {
        self.file_system
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/prepared/{relative}")).map_err(|error| FixtureError::new(error.to_string()))
    }

    fn seed_file(&self, relative: &str, _bytes: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        Ok(FixtureSupport::Supported(self.path(relative)?))
    }

    fn seed_empty_directory(&self, relative: &str) -> FixtureResult<FixtureSupport<Path>> {
        Ok(FixtureSupport::Supported(self.path(relative)?))
    }

    fn checksum_failure_case(&self, relative: &str) -> FixtureResult<FixtureSupport<Path>> {
        Ok(FixtureSupport::Supported(self.path(relative)?))
    }

    fn copy_fast_path_case(&self, _method: CopyMethod) -> FixtureResult<FixtureSupport<CopyFixtureCase>> {
        Ok(FixtureSupport::Supported(CopyFixtureCase::new(
            self.path("native-source")?,
            self.path("native-target")?,
            CopyOptions::file(),
        )))
    }
}

impl FileSystemFixture for DefaultSyncFixture<'_> {
    fn teardown(&self) -> FixtureResult<()> {
        Ok(())
    }

    fn file_system(&self) -> &FileSystem {
        self.file_system
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/defaults/{relative}")).map_err(|error| FixtureError::new(error.to_string()))
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
    assert!(matches!(fixture.read_file(&path), Ok(FixtureSupport::Unsupported)));
    assert!(matches!(
        fixture.resource_version(&path),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.seed_empty_directory("directory"),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(fixture.seed_symlink("link"), Ok(FixtureSupport::Unsupported)));
    assert!(matches!(
        fixture.copy_fast_path_case(CopyMethod::Native),
        Ok(FixtureSupport::Unsupported)
    ));
}

/// Default preparation hooks distinguish unavailable evidence from ready
/// requests across every scenario family.
#[test]
fn test_file_system_fixture_default_preparations_cover_scenarios() {
    let memory = MemoryFixture::new();
    let fixture = DefaultSyncFixture {
        file_system: memory.file_system(),
    };

    for scenario in [
        CopyScenario::Basic,
        CopyScenario::AtomicFile,
        CopyScenario::DurableFile,
        CopyScenario::AtomicTree,
        CopyScenario::DurableTree,
        CopyScenario::ServerSide,
        CopyScenario::Conflict,
    ] {
        assert!(
            matches!(
                fixture.prepare_copy(scenario, "source", "target", b"payload").unwrap(),
                FixturePreparation::Unavailable { .. }
            ),
            "{scenario:?}"
        );
    }

    for scenario in [DeleteScenario::Basic, DeleteScenario::IfMatch] {
        assert!(
            matches!(
                fixture.prepare_delete(scenario, "delete", b"payload").unwrap(),
                FixturePreparation::Unavailable { .. }
            ),
            "{scenario:?}"
        );
    }

    for scenario in [
        ReadScenario::Basic,
        ReadScenario::Range,
        ReadScenario::RangeLimit,
        ReadScenario::IfMatchCurrent,
        ReadScenario::IfMatchStale,
        ReadScenario::IfNoneMatchCurrent,
        ReadScenario::IfNoneMatchStale,
        ReadScenario::Checksum,
        ReadScenario::ChecksumCorruption,
    ] {
        assert!(
            matches!(
                fixture.prepare_read(scenario, "read", b"payload").unwrap(),
                FixturePreparation::Unavailable { .. }
            ),
            "{scenario:?}"
        );
    }

    for scenario in [
        WriteScenario::Replace,
        WriteScenario::CreateConflict,
        WriteScenario::Append,
        WriteScenario::AtomicReplace,
        WriteScenario::IfAbsent,
        WriteScenario::IfMatch,
    ] {
        assert!(
            matches!(
                fixture.prepare_write(scenario, "write", b"payload").unwrap(),
                FixturePreparation::Unavailable { .. }
            ),
            "{scenario:?}"
        );
    }

    for scenario in [WriteScenario::Create, WriteScenario::Abort, WriteScenario::Durable] {
        assert!(
            matches!(
                fixture.prepare_write(scenario, "write", b"payload").unwrap(),
                FixturePreparation::Ready(_)
            ),
            "{scenario:?}"
        );
    }
}

/// Provider setup hooks let default preparations build every applicable case.
#[test]
fn test_file_system_fixture_preparations_build_supported_cases() {
    let memory = MemoryFixture::new();
    let fixture = PreparedSyncFixture {
        file_system: memory.file_system(),
    };

    for scenario in [
        CopyScenario::Basic,
        CopyScenario::AtomicFile,
        CopyScenario::DurableFile,
        CopyScenario::AtomicTree,
        CopyScenario::DurableTree,
        CopyScenario::ServerSide,
        CopyScenario::Conflict,
    ] {
        assert!(
            matches!(
                fixture.prepare_copy(scenario, "source", "target", b"payload").unwrap(),
                FixturePreparation::Ready(_)
            ),
            "{scenario:?}"
        );
    }

    for scenario in [DeleteScenario::Basic, DeleteScenario::IfMatch] {
        assert!(
            matches!(
                fixture.prepare_delete(scenario, "delete", b"payload").unwrap(),
                FixturePreparation::Ready(_)
            ),
            "{scenario:?}"
        );
    }

    for scenario in [
        ReadScenario::Basic,
        ReadScenario::Range,
        ReadScenario::RangeLimit,
        ReadScenario::IfMatchCurrent,
        ReadScenario::IfMatchStale,
        ReadScenario::IfNoneMatchCurrent,
        ReadScenario::IfNoneMatchStale,
        ReadScenario::Checksum,
        ReadScenario::ChecksumCorruption,
    ] {
        assert!(
            matches!(
                fixture.prepare_read(scenario, "read", b"payload").unwrap(),
                FixturePreparation::Ready(_)
            ),
            "{scenario:?}"
        );
    }

    for scenario in [
        WriteScenario::Replace,
        WriteScenario::CreateConflict,
        WriteScenario::Append,
        WriteScenario::AtomicReplace,
        WriteScenario::Create,
        WriteScenario::Abort,
        WriteScenario::Durable,
    ] {
        assert!(
            matches!(
                fixture.prepare_write(scenario, "write", b"payload").unwrap(),
                FixturePreparation::Ready(_)
            ),
            "{scenario:?}"
        );
    }

    for scenario in [WriteScenario::IfAbsent, WriteScenario::IfMatch] {
        assert!(
            matches!(
                fixture.prepare_write(scenario, "write", b"payload").unwrap(),
                FixturePreparation::Unavailable { .. }
            ),
            "{scenario:?}"
        );
    }
}
