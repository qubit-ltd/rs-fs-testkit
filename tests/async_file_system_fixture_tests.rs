// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![cfg(feature = "async")]

use std::future::Future;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

pub use ::qubit_fs_testkit;
use qubit_fs::AsyncFileSystem;
use qubit_fs::copy::CopyMethod;
use qubit_fs::copy::CopyOptions;
use qubit_fs::path::Path;
use qubit_fs::temp::TempOptions;
use qubit_fs_testkit as testkit;
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

#[test]
fn test_async_temp_resources_do_not_reuse_a_live_path() {
    run_controlled(async {
        let file_fixture = AsyncMemoryFixture::new();
        let options = TempOptions::default().with_parent(Some(Path::root()));
        let mut file_a = file_fixture
            .file_system()
            .create_temp_file(options.clone())
            .await
            .expect("create file A");
        let mut file_b = file_fixture
            .file_system()
            .create_temp_file(options.clone())
            .await
            .expect("create file B");
        let file_b_path = file_b.path().clone();
        assert!(matches!(
            file_fixture
                .write_file_out_of_band(&file_b_path, b"file sentinel")
                .await
                .expect("seed file B"),
            FixtureSupport::Supported(())
        ));
        file_a.cleanup().await.expect("cleanup file A");
        let mut file_c = file_fixture
            .file_system()
            .create_temp_file(options.clone())
            .await
            .expect("create file C");
        let file_c_path = file_c.path().clone();
        assert_ne!(
            file_b_path, file_c_path,
            "file C must not reuse live file B's path"
        );
        assert_eq!(
            file_fixture
                .read_file(&file_b_path)
                .await
                .expect("observe file B"),
            qubit_fs_testkit::FixtureSupport::Supported(b"file sentinel".to_vec()),
            "creating file C must preserve file B's bytes",
        );
        file_b.cleanup().await.expect("cleanup file B");
        file_c.cleanup().await.expect("cleanup file C");

        let directory_fixture = AsyncMemoryFixture::new();
        let mut directory_a = directory_fixture
            .file_system()
            .create_temp_directory(options.clone())
            .await
            .expect("create directory A");
        // These live resources make the namespace size shrink back to B's
        // original index after they and A are cleaned, despite B's nested
        // sentinel adding namespace entries.
        let mut directory_x = directory_fixture
            .file_system()
            .create_temp_directory(options.clone())
            .await
            .expect("create directory X");
        let mut directory_y = directory_fixture
            .file_system()
            .create_temp_directory(options.clone())
            .await
            .expect("create directory Y");
        let mut directory_b = directory_fixture
            .file_system()
            .create_temp_directory(options.clone())
            .await
            .expect("create directory B");
        let directory_b_path = directory_b.path().clone();
        let sentinel_path = Path::parse(&format!("{}/nested/sentinel", directory_b_path.as_str()))
            .expect("build nested sentinel path");
        assert!(matches!(
            directory_fixture
                .write_file_out_of_band(&sentinel_path, b"directory sentinel")
                .await
                .expect("seed nested directory B sentinel"),
            FixtureSupport::Supported(())
        ));
        directory_a.cleanup().await.expect("cleanup directory A");
        directory_x.cleanup().await.expect("cleanup directory X");
        directory_y.cleanup().await.expect("cleanup directory Y");
        let mut directory_c = directory_fixture
            .file_system()
            .create_temp_directory(options)
            .await
            .expect("create directory C");
        let directory_c_path = directory_c.path().clone();
        assert_ne!(
            directory_b_path, directory_c_path,
            "directory C must not reuse live directory B's path"
        );
        assert_eq!(
            directory_fixture
                .read_file(&sentinel_path)
                .await
                .expect("observe nested sentinel"),
            qubit_fs_testkit::FixtureSupport::Supported(b"directory sentinel".to_vec()),
            "creating directory C must preserve directory B's nested bytes",
        );
        directory_b.cleanup().await.expect("cleanup directory B");
        directory_c.cleanup().await.expect("cleanup directory C");
        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .expect("temporary resources remain isolated while live");
}

mod common;

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
fn test_async_memory_resource_versions_follow_out_of_band_publication() {
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
