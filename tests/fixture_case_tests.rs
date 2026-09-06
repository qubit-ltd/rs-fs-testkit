// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================

mod common;

use common::MemoryFixture;
use qubit_fs::FileSystem;
use qubit_fs::copy::CopyMethod;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::path::Path;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureCase;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;

/// Fixture that relies on every synchronous optional hook default.
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

#[test]
fn fixture_cases_have_stable_distinct_meanings() {
    let cases = [
        FixtureCase::Capability(FileSystemCapability::Read),
        FixtureCase::CopyOverwrite,
        FixtureCase::CopyTree,
        FixtureCase::ReadIfMatch,
        FixtureCase::ReadIfNoneMatch,
        FixtureCase::WriteIfAbsent,
        FixtureCase::WriteIfMatch,
        FixtureCase::DeleteIfMatch,
    ];

    for (index, case) in cases.iter().enumerate() {
        assert!(
            cases[..index].iter().all(|previous| previous != case),
            "fixture case was duplicated: {case:?}"
        );
    }
}

#[test]
fn synchronous_fixture_defaults_report_optional_probes_as_unsupported() {
    let memory = MemoryFixture::new();
    let fixture = DefaultSyncFixture {
        file_system: memory.file_system(),
    };
    let path = fixture.path("entry").expect("build default path");

    assert!(matches!(
        fixture.case_support(FixtureCase::ReadIfMatch),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(!fixture.copy_fallback_only());
    assert!(matches!(
        fixture.exists_out_of_band(&path),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.write_file_out_of_band(&path, b"bytes"),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.stale_resource_version(&path),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.checksum_failure_case("corrupt"),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.teardown(),
        Ok(FixtureSupport::Unsupported)
    ));
    assert!(matches!(
        fixture.copy_fast_path_case(CopyMethod::Native),
        Ok(FixtureSupport::Unsupported)
    ));
}

#[cfg(feature = "async")]
mod asynchronous_defaults {
    use super::*;
    use std::future::Future;
    use std::task::Context;
    use std::task::Poll;
    use std::task::Waker;

    use common::AsyncMemoryFixture;
    use qubit_fs::AsyncFileSystem;
    use qubit_fs_testkit::AsyncFileSystemFixture;
    use qubit_fs_testkit::FixtureFuture;

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

    fn poll_fixture_future<T>(future: FixtureFuture<'_, T>) -> FixtureResult<T> {
        let mut future = Box::pin(future);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("default fixture future unexpectedly suspended"),
        }
    }

    #[test]
    fn asynchronous_fixture_defaults_report_optional_probes_as_unsupported() {
        let memory = AsyncMemoryFixture::new();
        let fixture = DefaultAsyncFixture {
            file_system: memory.file_system(),
        };
        let path = fixture.path("entry").expect("build default path");

        assert!(matches!(
            fixture.case_support(FixtureCase::ReadIfMatch),
            Ok(FixtureSupport::Unsupported)
        ));
        assert!(!fixture.copy_fallback_only());
        assert!(matches!(
            poll_fixture_future(fixture.exists_out_of_band(&path)),
            Ok(FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            poll_fixture_future(fixture.write_file_out_of_band(&path, b"bytes")),
            Ok(FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            poll_fixture_future(fixture.stale_resource_version(&path)),
            Ok(FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            poll_fixture_future(fixture.checksum_failure_case("corrupt")),
            Ok(FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            poll_fixture_future(fixture.teardown()),
            Ok(FixtureSupport::Unsupported)
        ));
        assert!(matches!(
            poll_fixture_future(fixture.copy_fast_path_case(CopyMethod::Native)),
            Ok(FixtureSupport::Unsupported)
        ));
    }
}
