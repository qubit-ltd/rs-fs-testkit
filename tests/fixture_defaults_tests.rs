// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;
pub use ::qubit_fs_testkit;
use qubit_fs::FileSystem;
use qubit_fs::copy::CopyMethod;
use qubit_fs::path::Path;
use qubit_fs_testkit as testkit;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;

use self::common::MemoryFixture;

/// Fixture that relies on every synchronous optional hook default.
struct DefaultSyncFixture<'a> {
    file_system: &'a FileSystem,
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

#[test]
fn test_synchronous_fixture_defaults_report_optional_probes_as_unsupported() {
    let memory = MemoryFixture::new();
    let fixture = DefaultSyncFixture {
        file_system: memory.file_system(),
    };
    let path = fixture.path("entry").expect("build default path");

    assert!(
        matches!(
            fixture.prepare_read(testkit::ReadScenario::IfMatchCurrent, "conditional", b"bytes"),
            Ok(testkit::FixturePreparation::Unavailable { .. })
        ),
        "prepare_read default must report unavailable"
    );
    assert!(!fixture.copy_fallback_only());
    assert!(
        matches!(fixture.exists_out_of_band(&path), Ok(FixtureSupport::Unsupported)),
        "exists_out_of_band default must be unsupported"
    );
    assert!(
        matches!(
            fixture.write_file_out_of_band(&path, b"bytes"),
            Ok(FixtureSupport::Unsupported)
        ),
        "write_file_out_of_band default must be unsupported"
    );
    assert!(
        matches!(fixture.stale_resource_version(&path), Ok(FixtureSupport::Unsupported)),
        "stale_resource_version default must be unsupported"
    );
    assert!(
        matches!(
            fixture.checksum_failure_case("corrupt"),
            Ok(FixtureSupport::Unsupported)
        ),
        "checksum_failure_case default must be unsupported"
    );
    fixture.teardown().expect("resource-free fixture teardown");
    assert!(
        matches!(
            fixture.copy_fast_path_case(CopyMethod::Native),
            Ok(FixtureSupport::Unsupported)
        ),
        "copy_fast_path_case default must be unsupported"
    );
}

#[cfg(feature = "async")]
mod asynchronous_defaults {
    use std::future::Future;
    use std::task::Context;
    use std::task::Poll;
    use std::task::Waker;

    use qubit_fs::AsyncFileSystem;
    use qubit_fs::copy::CopyMethod;
    use qubit_fs::path::Path;
    use qubit_fs_testkit as testkit;
    use qubit_fs_testkit::AsyncFileSystemFixture;
    use qubit_fs_testkit::FixtureFuture;

    use crate::FixtureError;
    use crate::FixtureResult;
    use crate::FixtureSupport;
    use crate::common::AsyncMemoryFixture;

    struct DefaultAsyncFixture<'a> {
        file_system: &'a AsyncFileSystem,
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

    /// Resolves an immediately-ready fixture future without an async runtime.
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
    fn test_asynchronous_fixture_defaults_report_optional_probes_as_unsupported() {
        let memory = AsyncMemoryFixture::new();
        let fixture = DefaultAsyncFixture {
            file_system: memory.file_system(),
        };
        let path = fixture.path("entry").expect("build default path");

        assert!(
            matches!(
                poll_fixture_future(fixture.prepare_read(
                    testkit::ReadScenario::IfMatchCurrent,
                    "conditional",
                    b"bytes"
                )),
                Ok(testkit::FixturePreparation::Unavailable { .. })
            ),
            "prepare_read default must report unavailable"
        );
        assert!(!fixture.copy_fallback_only());
        assert!(
            matches!(
                poll_fixture_future(fixture.exists_out_of_band(&path)),
                Ok(FixtureSupport::Unsupported)
            ),
            "exists_out_of_band default must be unsupported"
        );
        assert!(
            matches!(
                poll_fixture_future(fixture.write_file_out_of_band(&path, b"bytes")),
                Ok(FixtureSupport::Unsupported)
            ),
            "write_file_out_of_band default must be unsupported"
        );
        assert!(
            matches!(
                poll_fixture_future(fixture.stale_resource_version(&path)),
                Ok(FixtureSupport::Unsupported)
            ),
            "stale_resource_version default must be unsupported"
        );
        assert!(
            matches!(
                poll_fixture_future(fixture.checksum_failure_case("corrupt")),
                Ok(FixtureSupport::Unsupported)
            ),
            "checksum_failure_case default must be unsupported"
        );
        poll_fixture_future(fixture.teardown()).expect("resource-free fixture teardown");
        assert!(
            matches!(
                poll_fixture_future(fixture.copy_fast_path_case(CopyMethod::Native)),
                Ok(FixtureSupport::Unsupported)
            ),
            "copy_fast_path_case default must be unsupported"
        );
    }
}
