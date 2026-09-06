// =============================================================================

#![cfg(feature = "async")]
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use std::future::Future;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use common::async_memory_file_system::run_controlled;
use common::AsyncMemoryFixture;
use qubit_fs::AsyncFileSystem;
use qubit_fs::copy::CopyMethod;
use qubit_fs::path::Path;
use qubit_fs_testkit::AsyncCopyCancellationStage;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;

/// Fixture that uses every asynchronous optional-hook default.
struct DefaultAsyncFixture<'a> {
    file_system: &'a AsyncFileSystem,
}

impl AsyncFileSystemFixture for DefaultAsyncFixture<'_> {
    fn file_system(&self) -> &AsyncFileSystem {
        self.file_system
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/defaults/{relative}"))
            .map_err(|error| FixtureError::new(error.to_string()))
    }
}

/// Polls a fixture future that completes without suspension.
fn poll_fixture_future<T>(
    future: impl Future<Output = FixtureResult<T>>,
) -> FixtureResult<T> {
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
        fixture.copy_cancellation_case(AsyncCopyCancellationStage::Reader),
        Ok(FixtureSupport::Unsupported)
    ));
}

#[test]
fn async_memory_resource_versions_follow_out_of_band_publication() {
    let fixture = AsyncMemoryFixture::new();
    let path = match run_controlled(fixture.seed_file("versioned", b"old"))
        .expect("seed versioned file")
    {
        FixtureSupport::Supported(path) => path,
        FixtureSupport::Unsupported => panic!("memory fixture must support file seeding"),
    };
    let first = match run_controlled(fixture.resource_version(&path))
        .expect("read initial resource version")
    {
        FixtureSupport::Supported(version) => version,
        FixtureSupport::Unsupported => panic!("seeded file must have a resource version"),
    };
    let _ = run_controlled(fixture.write_file_out_of_band(&path, b"new"))
        .expect("publish updated versioned file");
    let second = match run_controlled(fixture.resource_version(&path))
        .expect("read updated resource version")
    {
        FixtureSupport::Supported(version) => version,
        FixtureSupport::Unsupported => panic!("updated file must have a resource version"),
    };
    assert_ne!(first, second);
    assert_eq!(
        match run_controlled(fixture.stale_resource_version(&path))
            .expect("read stale resource version")
        {
            FixtureSupport::Supported(version) => version,
            FixtureSupport::Unsupported => panic!("updated file must have stale version"),
        },
        first
    );
}
