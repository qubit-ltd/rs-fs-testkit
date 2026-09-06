// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================

mod common;

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use common::MemoryFault;
use common::MemoryFixture;
use qubit_fs::path::Path;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;

/// A small adapter used to observe teardown independently of facade cleanup.
struct TeardownProbe {
    inner: MemoryFixture,
    calls: Arc<AtomicUsize>,
    teardown: TeardownBehavior,
}

enum TeardownBehavior {
    Succeeds,
    Fails,
    Panics,
}

impl TeardownProbe {
    fn new(inner: MemoryFixture, result: Result<FixtureSupport<()>, FixtureError>) -> Self {
        Self {
            inner,
            calls: Arc::new(AtomicUsize::new(0)),
            teardown: if result.is_ok() {
                TeardownBehavior::Succeeds
            } else {
                TeardownBehavior::Fails
            },
        }
    }

    fn panicking(inner: MemoryFixture) -> Self {
        Self {
            inner,
            calls: Arc::new(AtomicUsize::new(0)),
            teardown: TeardownBehavior::Panics,
        }
    }
}

impl FileSystemFixture for TeardownProbe {
    fn file_system(&self) -> &qubit_fs::FileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn teardown(&self) -> FixtureResult<FixtureSupport<()>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        match &self.teardown {
            TeardownBehavior::Succeeds => Ok(FixtureSupport::Supported(())),
            TeardownBehavior::Fails => Err(FixtureError::new("teardown failed")),
            TeardownBehavior::Panics => panic!("teardown panic payload"),
        }
    }
}

/// A teardown failure does not replace the panic raised by the contract body.
#[test]
fn test_body_panic_is_preserved_when_teardown_also_fails() {
    let fixture = TeardownProbe::new(
        MemoryFixture::with_fault(MemoryFault::WriteDropsBytes),
        Err(FixtureError::new("teardown failed")),
    );
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        FileSystemContractSuite::new(&fixture).assert_contract(FileSystemContract::Write);
    }));
    let payload = result.expect_err("the write fault must fail the selected contract");
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|value| (*value).to_owned())
        })
        .unwrap_or_default();
    assert!(message.contains("fixture.read_file"), "unexpected panic: {message}");
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 1);
}

/// The body panic payload remains primary when teardown itself panics.
#[test]
fn test_body_panic_is_preserved_when_teardown_panics() {
    let fixture = TeardownProbe::panicking(MemoryFixture::with_fault(
        MemoryFault::WriteDropsBytes,
    ));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        FileSystemContractSuite::new(&fixture).assert_contract(FileSystemContract::Write);
    }));
    let payload = result.expect_err("the write fault must fail the selected contract");
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|value| (*value).to_owned())
        })
        .unwrap_or_default();
    assert!(message.contains("fixture.read_file"), "unexpected panic: {message}");
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 1);
}

/// A facade without Delete still invokes the fixture teardown hook.
#[test]
fn test_missing_delete_does_not_skip_fixture_teardown() {
    let fixture = TeardownProbe::new(
        MemoryFixture::without_delete(),
        Ok(FixtureSupport::Supported(())),
    );
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.assert_error_context();
    suite.finish();
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 1);
}

/// Cleanup keeps failed resources in the ledger so a later finish can retry them.
#[test]
fn test_cleanup_failure_is_retained_for_retry() {
    let fixture = MemoryFixture::with_fault(MemoryFault::DeleteNoOp);
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.assert_write();
    let first = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| suite.finish()));
    assert!(
        first.is_err(),
        "the injected delete failure must be reported"
    );
    let first_message = first
        .as_ref()
        .err()
        .and_then(|payload| payload.downcast_ref::<FixtureError>())
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(
        first_message.contains("[fs-testkit:cleanup/delete]"),
        "unexpected cleanup panic: {first_message}"
    );
    let retained = fixture.entry_count();
    assert!(retained > 0, "failed cleanup must retain resources");

    let second = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| suite.finish()));
    assert!(second.is_err(), "finish must retry retained resources");
    assert_eq!(fixture.entry_count(), retained);

    fixture.set_fault(MemoryFault::None);
    suite.finish();
    assert!(
        fixture.is_empty(),
        "a recovered delete fault must drain retained resources"
    );
}

/// Cleanup attempts all resources even when the provider rejects each delete.
#[test]
fn test_cleanup_continues_after_an_intermediate_failure() {
    let fixture = MemoryFixture::with_fault(MemoryFault::DeleteNoOp);
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.assert_write();
    let before = fixture.entry_count();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| suite.finish()));
    assert!(result.is_err());
    assert!(
        before >= 2,
        "the write phase must prepare multiple resources"
    );
    assert!(
        fixture.delete_attempt_count() >= before,
        "cleanup must attempt every retained resource"
    );
    assert_eq!(fixture.entry_count(), before);
}
