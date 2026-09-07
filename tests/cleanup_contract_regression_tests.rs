// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================

mod common;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_fs::FileSystem;
use qubit_fs::path::Path;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;

use self::common::MemoryFault;
use self::common::MemoryFixture;
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

/// Borrowed run evidence must allow taking the original panic exactly once.
#[test]
fn test_borrowed_cleanup_failure_exposes_original_panic() {
    let fixture = TeardownProbe::panicking(MemoryFixture::new());
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::ErrorContext);
    let failure = run.cleanup().failures().first().expect("teardown panic retained");
    let payload = failure.take_panic_payload().expect("original payload available");
    assert_eq!(payload.downcast_ref::<&str>(), Some(&"teardown panic payload"));
    assert!(failure.take_panic_payload().is_none(), "payload transfers only once");
    assert!(!run.requirements_satisfied(), "taking evidence must not erase failure");
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
    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn teardown(&self) -> FixtureResult<()> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        match &self.teardown {
            TeardownBehavior::Succeeds => Ok(()),
            TeardownBehavior::Fails => Err(FixtureError::new("teardown failed")),
            TeardownBehavior::Panics => panic!("teardown panic payload"),
        }
    }
}

/// A provider wrapper that fails observation after leaving the written target.
struct BodyAndCleanupProbe {
    inner: MemoryFixture,
}

impl FileSystemFixture for BodyAndCleanupProbe {
    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn read_file(&self, _path: &Path) -> FixtureResult<FixtureSupport<Vec<u8>>> {
        Ok(FixtureSupport::Supported(b"unexpected body bytes".to_vec()))
    }

    fn teardown(&self) -> FixtureResult<()> {
        Ok(())
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
        FileSystemContractSuite::new(&fixture)
            .run_contract(FileSystemContract::Write)
            .assert_satisfied();
    }));
    let payload = result.expect_err("the write fault must fail the selected contract");
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|value| (*value).to_owned()))
        .unwrap_or_default();
    assert!(message.contains("fixture.read_file"), "unexpected panic: {message}");
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 1);
}

/// The body panic payload remains primary when teardown itself panics.
#[test]
fn test_body_panic_is_preserved_when_teardown_panics() {
    let fixture = TeardownProbe::panicking(MemoryFixture::with_fault(MemoryFault::WriteDropsBytes));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        FileSystemContractSuite::new(&fixture)
            .run_contract(FileSystemContract::Write)
            .assert_satisfied();
    }));
    let payload = result.expect_err("the write fault must fail the selected contract");
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|value| (*value).to_owned()))
        .unwrap_or_default();
    assert!(message.contains("fixture.read_file"), "unexpected panic: {message}");
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 1);
}

/// A body panic remains primary when cleanup also reports a failed delete.
#[test]
fn test_body_panic_is_preserved_when_cleanup_fails() {
    let fixture = BodyAndCleanupProbe {
        inner: MemoryFixture::with_fault(MemoryFault::DeleteNoOp),
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        FileSystemContractSuite::new(&fixture)
            .run_contract(FileSystemContract::Write)
            .assert_satisfied();
    }));
    let payload = result.expect_err("the observation fault must fail the selected contract");
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|value| (*value).to_owned()))
        .unwrap_or_default();
    assert!(
        message.contains("writer contract: write was not published"),
        "unexpected panic: {message}"
    );
    assert!(fixture.inner.entry_count() > 0, "failed cleanup must retain resources");
}

/// A facade without Delete still invokes the fixture teardown hook.
#[test]
fn test_missing_delete_does_not_skip_fixture_teardown() {
    let fixture = TeardownProbe::new(MemoryFixture::without_delete(), Ok(FixtureSupport::Supported(())));
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.run_contract(FileSystemContract::ErrorContext).assert_satisfied();
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 1);
}

/// Cleanup keeps failed resources in the ledger so a later finish can retry
/// them.
#[test]
fn test_cleanup_failure_is_retained_for_retry() {
    let fixture = MemoryFixture::with_fault(MemoryFault::DeleteNoOp);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Write);
    assert!(
        !run.requirements_satisfied(),
        "initial cleanup failure must remain visible"
    );
    assert!(fixture.entry_count() > 0, "write phase must prepare resources");
    let first = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| suite.finish()));
    assert!(first.is_err(), "the injected delete failure must be reported");
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
    let run = suite.run_contract(FileSystemContract::Write);
    assert!(
        !run.requirements_satisfied(),
        "initial cleanup failure must remain visible"
    );
    let before = fixture.entry_count();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| suite.finish()));
    assert!(result.is_err());
    assert!(before >= 2, "the write phase must prepare multiple resources");
    assert!(
        fixture.delete_attempt_count() >= before,
        "cleanup must attempt every retained resource"
    );
    assert_eq!(fixture.entry_count(), before);
}

#[test]
fn test_cleanup_retains_resources_for_stat_and_delete_failures() {
    let mut faults = vec![MemoryFault::CleanupStatError];
    faults.extend([MemoryFault::CleanupDeleteError, MemoryFault::CleanupDeletePanic]);

    for fault in faults {
        let fixture = MemoryFixture::with_fault(MemoryFault::None);
        let mut suite = FileSystemContractSuite::new(&fixture);
        fixture.set_fault(fault);
        let run = suite.run_contract(FileSystemContract::Write);
        assert!(
            !run.requirements_satisfied(),
            "initial cleanup failure must remain visible"
        );
        let initial = fixture.entry_count();
        assert!(initial > 0, "write phase must prepare resources");

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| suite.finish()));
        assert!(result.is_err(), "cleanup fault must be reported: {fault:?}");
        assert_eq!(
            fixture.entry_count(),
            initial,
            "cleanup fault must retain every resource: {fault:?}"
        );

        fixture.set_fault(MemoryFault::None);
        suite.finish();
        assert!(
            fixture.is_empty(),
            "retry after recovering cleanup fault must drain resources: {fault:?}"
        );
    }
}

/// Execution and teardown failures remain independently inspectable.
#[test]
fn test_run_retains_body_and_teardown_sources() {
    let fixture = TeardownProbe::new(
        MemoryFixture::with_fault(MemoryFault::WriteDropsBytes),
        Err(FixtureError::new("teardown failed")),
    );
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Write);
    assert!(!run.failures().is_empty());
    assert_eq!(run.cleanup().attempts(), 1);
    assert!(!run.cleanup().failures().is_empty());
    assert!(std::error::Error::source(&run.cleanup().failures()[0]).is_some());
    assert!(!run.requirements_satisfied());
}
