// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![cfg(feature = "async")]

pub use ::qubit_fs_testkit;
use qubit_fs_testkit as testkit;
mod common;
use std::future::Future;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_fs::AsyncFileSystem;
use qubit_fs::copy::CopyMethod;
use qubit_fs::copy::CopyOptions;
use qubit_fs::path::Path;
use qubit_fs_testkit::AsyncCopyCancellationStage;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::CopyFixtureCase;
use qubit_fs_testkit::CopyScenario;
use qubit_fs_testkit::DeleteScenario;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixturePreparation;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;
use qubit_fs_testkit::ReadScenario;
use qubit_fs_testkit::WriteScenario;

use self::common::AsyncMemoryFixture;
use self::common::async_memory_file_system::run_controlled;
/// Fixture that uses every asynchronous optional-hook default.
struct DefaultAsyncFixture<'a> {
    file_system: &'a AsyncFileSystem,
}

/// Fixture implementing independent setup hooks for the trait defaults.
struct PreparedAsyncFixture<'a> {
    file_system: &'a AsyncFileSystem,
}

impl AsyncFileSystemFixture for PreparedAsyncFixture<'_> {
    fn teardown(&self) -> testkit::FixtureFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }

    fn file_system(&self) -> &AsyncFileSystem {
        self.file_system
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/prepared/{relative}")).map_err(|error| FixtureError::new(error.to_string()))
    }

    fn seed_file<'a>(
        &'a self,
        relative: &'a str,
        _bytes: &'a [u8],
    ) -> testkit::FixtureFuture<'a, FixtureSupport<Path>> {
        Box::pin(async move { Ok(FixtureSupport::Supported(self.path(relative)?)) })
    }

    fn seed_empty_directory<'a>(&'a self, relative: &'a str) -> testkit::FixtureFuture<'a, FixtureSupport<Path>> {
        Box::pin(async move { Ok(FixtureSupport::Supported(self.path(relative)?)) })
    }

    fn checksum_failure_case<'a>(&'a self, relative: &'a str) -> testkit::FixtureFuture<'a, FixtureSupport<Path>> {
        Box::pin(async move { Ok(FixtureSupport::Supported(self.path(relative)?)) })
    }

    fn copy_fast_path_case<'a>(
        &'a self,
        _method: CopyMethod,
    ) -> testkit::FixtureFuture<'a, FixtureSupport<CopyFixtureCase>> {
        Box::pin(async move {
            Ok(FixtureSupport::Supported(CopyFixtureCase::new(
                self.path("native-source")?,
                self.path("native-target")?,
                CopyOptions::file(),
            )))
        })
    }
}

impl AsyncFileSystemFixture for DefaultAsyncFixture<'_> {
    fn teardown(&self) -> testkit::FixtureFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }

    fn file_system(&self) -> &AsyncFileSystem {
        self.file_system
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/defaults/{relative}")).map_err(|error| FixtureError::new(error.to_string()))
    }
}

/// Polls a fixture future that completes without suspension.
fn poll_fixture_future<T>(future: impl Future<Output = FixtureResult<T>>) -> FixtureResult<T> {
    let mut future = Box::pin(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(result) => result,
        Poll::Pending => {
            panic!("default fixture future unexpectedly suspended")
        }
    }
}

/// Optional asynchronous fixture hooks report unavailable until a provider opts
/// in.
#[test]
fn test_async_file_system_fixture_defaults_are_unsupported() {
    let memory = AsyncMemoryFixture::new();
    let fixture = DefaultAsyncFixture {
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
        poll_fixture_future(fixture.seed_file("entry", b"bytes")),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        poll_fixture_future(fixture.read_file(&path)),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        poll_fixture_future(fixture.resource_version(&path)),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        poll_fixture_future(fixture.seed_empty_directory("directory")),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        poll_fixture_future(fixture.seed_symlink("link")),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        poll_fixture_future(fixture.copy_fast_path_case(CopyMethod::Native)),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        poll_fixture_future(fixture.prepare_copy_cancellation(AsyncCopyCancellationStage::Reader, "cancel-reader")),
        Ok(FixtureSupport::Unsupported)
    ));
}

/// Default asynchronous preparation hooks classify every scenario with no
/// provider-specific setup hooks.
#[test]
fn test_async_file_system_fixture_default_preparations_cover_scenarios() {
    let memory = AsyncMemoryFixture::new();
    let fixture = DefaultAsyncFixture {
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
                poll_fixture_future(fixture.prepare_copy(scenario, "source", "target", b"payload"))
                    .expect("prepare default copy"),
                FixturePreparation::Unavailable { .. }
            ),
            "{scenario:?}"
        );
    }

    for scenario in [DeleteScenario::Basic, DeleteScenario::IfMatch] {
        assert!(
            matches!(
                poll_fixture_future(fixture.prepare_delete(scenario, "delete", b"payload"))
                    .expect("prepare default delete"),
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
                poll_fixture_future(fixture.prepare_read(scenario, "read", b"payload")).expect("prepare default read"),
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
                poll_fixture_future(fixture.prepare_write(scenario, "write", b"payload"))
                    .expect("prepare default write"),
                FixturePreparation::Unavailable { .. }
            ),
            "{scenario:?}"
        );
    }

    for scenario in [WriteScenario::Create, WriteScenario::Abort, WriteScenario::Durable] {
        assert!(
            matches!(
                poll_fixture_future(fixture.prepare_write(scenario, "write", b"payload"))
                    .expect("prepare default write"),
                FixturePreparation::Ready(_)
            ),
            "{scenario:?}"
        );
    }
}

/// Provider setup hooks let asynchronous defaults build applicable cases.
#[test]
fn test_async_file_system_fixture_preparations_build_supported_cases() {
    let memory = AsyncMemoryFixture::new();
    let fixture = PreparedAsyncFixture {
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
                poll_fixture_future(fixture.prepare_copy(scenario, "source", "target", b"payload"))
                    .expect("prepare default copy"),
                FixturePreparation::Ready(_)
            ),
            "{scenario:?}"
        );
    }

    for scenario in [DeleteScenario::Basic, DeleteScenario::IfMatch] {
        assert!(
            matches!(
                poll_fixture_future(fixture.prepare_delete(scenario, "delete", b"payload"))
                    .expect("prepare default delete"),
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
                poll_fixture_future(fixture.prepare_read(scenario, "read", b"payload")).expect("prepare default read"),
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
                poll_fixture_future(fixture.prepare_write(scenario, "write", b"payload"))
                    .expect("prepare default write"),
                FixturePreparation::Ready(_)
            ),
            "{scenario:?}"
        );
    }

    for scenario in [WriteScenario::IfAbsent, WriteScenario::IfMatch] {
        assert!(
            matches!(
                poll_fixture_future(fixture.prepare_write(scenario, "write", b"payload"))
                    .expect("prepare default write"),
                FixturePreparation::Unavailable { .. }
            ),
            "{scenario:?}"
        );
    }
}

#[test]
fn async_memory_resource_versions_follow_out_of_band_publication() {
    let fixture = AsyncMemoryFixture::new();
    let path = match run_controlled(fixture.seed_file("versioned", b"old")).expect("seed versioned file") {
        FixtureSupport::Supported(path) => path,
        FixtureSupport::Unsupported => panic!("memory fixture must support file seeding"),
    };
    let first = match run_controlled(fixture.resource_version(&path)).expect("read initial resource version") {
        FixtureSupport::Supported(version) => version,
        FixtureSupport::Unsupported => panic!("seeded file must have a resource version"),
    };
    let _ = run_controlled(fixture.write_file_out_of_band(&path, b"new")).expect("publish updated versioned file");
    let second = match run_controlled(fixture.resource_version(&path)).expect("read updated resource version") {
        FixtureSupport::Supported(version) => version,
        FixtureSupport::Unsupported => panic!("updated file must have a resource version"),
    };
    assert_ne!(first, second);
    assert_eq!(
        match run_controlled(fixture.stale_resource_version(&path)).expect("read stale resource version") {
            FixtureSupport::Supported(version) => version,
            FixtureSupport::Unsupported => panic!("updated file must have stale version"),
        },
        first
    );
}
