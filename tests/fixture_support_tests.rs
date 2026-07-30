// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use std::future::Future;
use std::task::{Context, Poll, Waker};

use qubit_fs::{CopyMethod, FileSystem, Path};
use qubit_fs_testkit::{
    AsyncCopyCancellationStage, AsyncCopyFixtureCase, AsyncFileSystemFixture, CopyFixtureCase,
    FileSystemFixture, FixtureError, FixtureResult, FixtureSupport,
};

use common::{AsyncMemoryFixture, MemoryFixture};

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

    fn list_prefix(&self, _: &Path, relative: &str) -> FixtureResult<String> {
        Ok(relative.to_owned())
    }
}

/// Fixture that uses every asynchronous optional-hook default.
struct DefaultAsyncFixture<'a> {
    file_system: &'a qubit_fs::AsyncFileSystem,
}

impl AsyncFileSystemFixture for DefaultAsyncFixture<'_> {
    fn file_system(&self) -> &qubit_fs::AsyncFileSystem {
        self.file_system
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/defaults/{relative}"))
            .map_err(|error| FixtureError::new(error.to_string()))
    }

    fn list_prefix(&self, _: &Path, relative: &str) -> FixtureResult<String> {
        Ok(relative.to_owned())
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

/// Verifies that setup failures remain distinguishable from unavailable probes.
#[test]
fn test_fixture_support_does_not_conflate_error_with_unsupported() {
    let unsupported: FixtureResult<FixtureSupport<Path>> = Ok(FixtureSupport::Unsupported);
    let failure: FixtureResult<FixtureSupport<Path>> = Err(FixtureError::new("setup failed"));

    assert!(matches!(unsupported, Ok(FixtureSupport::Unsupported)));
    assert!(failure.is_err());
}

/// Optional synchronous fixture hooks report that their probes are unavailable
/// until a provider opts in.
#[test]
fn test_synchronous_fixture_defaults_are_unsupported() {
    let memory = MemoryFixture::new();
    let fixture = DefaultSyncFixture {
        file_system: memory.file_system(),
    };
    let path = fixture.path("entry").expect("build default path");

    assert!(matches!(
        fixture.seed_file("entry", b"bytes"),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.read_file(&path),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.copy_fast_path_case(CopyMethod::Native),
        Ok(FixtureSupport::Unsupported)
    ));
}

/// Optional asynchronous fixture hooks report that their probes are unavailable
/// until a provider opts in.
#[test]
fn test_asynchronous_fixture_defaults_are_unsupported() {
    let memory = AsyncMemoryFixture::new();
    let fixture = DefaultAsyncFixture {
        file_system: memory.file_system(),
    };
    let path = fixture.path("entry").expect("build default path");

    assert!(matches!(
        poll_fixture_future(fixture.seed_file("entry", b"bytes")),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        poll_fixture_future(fixture.read_file(&path)),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.copy_cancellation_case(AsyncCopyCancellationStage::Reader),
        Ok(FixtureSupport::Unsupported)
    ));
}

/// Fixture errors retain their message and optional source without exposing
/// source diagnostics through debug formatting.
#[test]
fn test_fixture_error_preserves_message_and_source() {
    let plain = FixtureError::new("plain failure");
    assert_eq!(plain.to_string(), "plain failure");
    assert!(std::error::Error::source(&plain).is_none());

    let sourced = FixtureError::with_source("outer failure", std::io::Error::other("inner"));
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

/// Prepared copy cases preserve their source, target, and options.
#[test]
fn test_copy_fixture_cases_expose_and_transfer_request_parts() {
    let source = Path::parse("/defaults/source").expect("valid source path");
    let target = Path::parse("/defaults/target").expect("valid target path");
    let case = CopyFixtureCase::new(source.clone(), target.clone(), Default::default());
    assert_eq!(case.source(), &source);
    assert_eq!(case.target(), &target);
    assert_eq!(case.options(), &qubit_fs::CopyOptions::default());
    let (actual_source, actual_target, actual_options) = case.into_parts();
    assert_eq!(actual_source, source);
    assert_eq!(actual_target, target);
    assert_eq!(actual_options, qubit_fs::CopyOptions::default());

    let async_case =
        AsyncCopyFixtureCase::new(actual_source.clone(), actual_target.clone(), actual_options);
    assert_eq!(async_case.source(), &actual_source);
    assert_eq!(async_case.target(), &actual_target);
    assert_eq!(async_case.options(), &qubit_fs::CopyOptions::default());
    let (source, target, options) = async_case.into_parts();
    assert_eq!(source, actual_source);
    assert_eq!(target, actual_target);
    assert_eq!(options, qubit_fs::CopyOptions::default());
}
