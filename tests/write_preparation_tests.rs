// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! A capability claim cannot be satisfied by skipping its positive scenario.

use qubit_fs as qfs;
use qubit_fs_testkit as testkit;

mod common;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_fs::FileSystem;
use qubit_fs::path::Path;
use qubit_fs::write::WriteDisposition;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixturePreparation;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::WriteFixtureCase;
use qubit_fs_testkit::WriteScenario;

use self::common::MemoryFixture;
/// Omits basic creation while preserving independent prepared write scenarios.
struct MissingCreationFixture(MemoryFixture);

impl FileSystemFixture for MissingCreationFixture {
    fn file_system(&self) -> &FileSystem {
        self.0.file_system()
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.0.path(relative)
    }
    fn teardown(&self) -> FixtureResult<()> {
        self.0.teardown()
    }
    fn read_file(&self, path: &Path) -> FixtureResult<testkit::FixtureSupport<Vec<u8>>> {
        self.0.read_file(path)
    }
    fn resource_version(&self, path: &Path) -> FixtureResult<testkit::FixtureSupport<qfs::metadata::ResourceVersion>> {
        self.0.resource_version(path)
    }
    fn stale_resource_version(
        &self,
        path: &Path,
    ) -> FixtureResult<testkit::FixtureSupport<qfs::metadata::ResourceVersion>> {
        self.0.stale_resource_version(path)
    }
    fn prepare_write(
        &self,
        scenario: WriteScenario,
        relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<FixturePreparation<WriteFixtureCase>> {
        if scenario == WriteScenario::Create {
            Ok(FixturePreparation::Unavailable {
                reason: "creation setup unavailable".to_owned(),
            })
        } else {
            self.0.prepare_write(scenario, relative, bytes)
        }
    }
}

/// Missing creation evidence must not suppress a prepared durable publication.
#[test]
fn test_missing_creation_does_not_block_independent_writes() {
    let fixture = MissingCreationFixture(MemoryFixture::with_all_capabilities());
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Write);
    for id in [
        ContractCheckId::WriteIfAbsent,
        ContractCheckId::WriteIfMatch,
        ContractCheckId::WriteAtomicReplaceExisting,
        ContractCheckId::WriteDurable,
    ] {
        let check = run
            .report()
            .checks()
            .iter()
            .find(|check| check.id() == id)
            .expect("write check registered");
        assert!(
            matches!(check.outcome(), ContractCheckOutcome::Passed),
            "{id}: {:?}",
            check.outcome()
        );
    }
    assert!(!run.requirements_satisfied(), "creation remains unverified");
    assert!(run.failures().is_empty());
    assert!(fixture.0.is_empty());
}

/// Creation conflict must execute even without a successful basic creation.
#[test]
fn test_creation_conflict_has_independent_rejection_evidence() {
    let fixture = MissingCreationFixture(MemoryFixture::with_all_capabilities());
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Write);
    let check = run
        .report()
        .checks()
        .iter()
        .find(|check| check.id().as_str() == "write/create-conflict")
        .expect("creation conflict must have its own registered check");
    assert!(matches!(check.outcome(), ContractCheckOutcome::RejectedAsExpected));
    assert!(fixture.0.is_empty());
}

/// Replacement must not depend on a target created by the basic write check.
#[test]
fn test_replacement_has_independent_publication_evidence() {
    let fixture = MissingCreationFixture(MemoryFixture::with_all_capabilities());
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Write);
    let check = run
        .report()
        .checks()
        .iter()
        .find(|check| check.id().as_str() == "write/replace")
        .expect("replacement must have its own registered check");
    assert!(matches!(check.outcome(), ContractCheckOutcome::Passed));
    assert!(fixture.0.is_empty());
}

/// Abort must have an independently prepared and observable request.
#[test]
fn test_abort_has_independent_evidence() {
    let fixture = MemoryFixture::new();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Write);
    let check = run
        .report()
        .checks()
        .iter()
        .find(|check| check.id().as_str() == "write/abort")
        .expect("abort must have its own registered check");
    assert!(matches!(check.outcome(), ContractCheckOutcome::Passed));
    assert!(fixture.is_empty());
}

/// Strong write guarantees must report their own failed evidence.
#[test]
fn test_write_guarantee_failures_have_precise_identities() {
    for (fault, id) in [
        (
            self::common::MemoryFault::AtomicReplaceKeepsOldBytes,
            ContractCheckId::WriteAtomicReplaceExisting,
        ),
        (
            self::common::MemoryFault::DurableWriteDropsBytes,
            ContractCheckId::WriteDurable,
        ),
        (
            self::common::MemoryFault::AppendOverwrites,
            ContractCheckId::AppendBasic,
        ),
        (
            self::common::MemoryFault::CreateNewOverwrites,
            ContractCheckId::WriteCreateConflict,
        ),
        (
            self::common::MemoryFault::ReplaceKeepsSuffix,
            ContractCheckId::WriteReplace,
        ),
        (self::common::MemoryFault::AbortLies, ContractCheckId::WriteAbort),
    ] {
        let fixture = MemoryFixture::with_fault(fault);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(FileSystemContract::Write);
        assert_eq!(run.failures()[0].check(), Some(id));
        assert!(fixture.is_empty());
    }
}

/// The asynchronous guarantee checks preserve the same failure ownership.
#[cfg(feature = "async")]
#[test]
fn test_async_write_guarantee_failures_have_precise_identities() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    for (fault, id) in [
        (
            AsyncMemoryFault::AtomicReplaceKeepsOldBytes,
            ContractCheckId::WriteAtomicReplaceExisting,
        ),
        (AsyncMemoryFault::DurableWriteDropsBytes, ContractCheckId::WriteDurable),
        (AsyncMemoryFault::AppendOverwrites, ContractCheckId::AppendBasic),
        (
            AsyncMemoryFault::CreateNewOverwrites,
            ContractCheckId::WriteCreateConflict,
        ),
        (AsyncMemoryFault::ReplaceKeepsSuffix, ContractCheckId::WriteReplace),
        (AsyncMemoryFault::AbortLies, ContractCheckId::WriteAbort),
    ] {
        let fixture = AsyncMemoryFixture::with_fault(fault);
        run_controlled(async {
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            let run = suite.run_contract(FileSystemContract::Write).await;
            assert_eq!(run.failures()[0].check(), Some(id));
            assert!(fixture.is_empty());
        });
    }
}

/// Ignoring a stale version must produce an attributed ordinary failure.
#[test]
fn test_if_match_violation_has_its_own_identity() {
    let fixture = MemoryFixture::with_fault(self::common::MemoryFault::IgnoreWriteIfMatch);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Write);
    assert_eq!(run.failures()[0].check(), Some(ContractCheckId::WriteIfMatch));
    assert!(fixture.is_empty());
}

/// The async driver also identifies the stale version violation precisely.
#[cfg(feature = "async")]
#[test]
fn test_async_if_match_violation_has_its_own_identity() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::IgnoreWriteIfMatch);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(FileSystemContract::Write).await;
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::WriteIfMatch));
        assert!(fixture.is_empty());
    });
}

/// Rejects only conditional preparation while basic creation remains usable.
struct FailingConditionalFixture(MemoryFixture);

impl FileSystemFixture for FailingConditionalFixture {
    fn file_system(&self) -> &FileSystem {
        self.0.file_system()
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.0.path(relative)
    }
    fn teardown(&self) -> FixtureResult<()> {
        self.0.teardown()
    }
    fn read_file(&self, path: &Path) -> FixtureResult<testkit::FixtureSupport<Vec<u8>>> {
        self.0.read_file(path)
    }
    fn prepare_write(
        &self,
        scenario: WriteScenario,
        relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<FixturePreparation<WriteFixtureCase>> {
        if scenario == WriteScenario::IfAbsent {
            Err(testkit::FixtureError::new("conditional setup failed"))
        } else {
            self.0.prepare_write(scenario, relative, bytes)
        }
    }
}

/// Conditional checks must use typed preparation and preserve its failure ID.
#[test]
fn test_if_absent_preparation_error_has_its_own_identity() {
    let fixture = FailingConditionalFixture(MemoryFixture::with_all_capabilities());
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Write);
    let failure = run.failures().first().expect("conditional setup must fail");
    assert_eq!(failure.check(), Some(ContractCheckId::WriteIfAbsent));
    assert!(std::error::Error::source(failure).is_some());
    assert!(fixture.0.is_empty());
}

/// An independent observer can expose corruption limited to boundary writes.
#[cfg(feature = "async")]
struct BoundaryObservationFixture(self::common::AsyncMemoryFixture);

#[cfg(feature = "async")]
impl testkit::AsyncFileSystemFixture for BoundaryObservationFixture {
    fn file_system(&self) -> &qfs::AsyncFileSystem {
        self.0.file_system()
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.0.path(relative)
    }
    fn teardown(&self) -> testkit::FixtureFuture<'_, ()> {
        self.0.teardown()
    }
    fn read_file<'a>(&'a self, path: &'a Path) -> testkit::FixtureFuture<'a, testkit::FixtureSupport<Vec<u8>>> {
        Box::pin(async move {
            if path.as_str().contains("write-limit-at") {
                Ok(testkit::FixtureSupport::Supported(b"corrupt boundary".to_vec()))
            } else {
                self.0.read_file(path).await
            }
        })
    }
}

/// Boundary publication needs independent bytes, not only successful execute.
#[cfg(feature = "async")]
#[test]
fn test_async_write_limit_verifies_published_bytes() {
    use qubit_fs::metadata::FileSystemLimit;
    use qubit_fs::metadata::FileSystemLimits;
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = BoundaryObservationFixture(AsyncMemoryFixture::with_limits(
        FileSystemLimits::unknown().with_max_write_bytes(FileSystemLimit::Maximum(8)),
    ));
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(FileSystemContract::Write).await;
        let failure = run.failures().first().expect("corrupt boundary must fail");
        assert_eq!(failure.check(), Some(ContractCheckId::WriteLimit));
        assert!(
            run.report()
                .checks()
                .iter()
                .any(|check| check.id() == ContractCheckId::WriteLimit
                    && matches!(check.outcome(), ContractCheckOutcome::Failed { .. }))
        );
        assert!(!run.requirements_satisfied());
        assert!(fixture.0.is_empty());
    });
}

struct UnavailableWriteFixture {
    inner: MemoryFixture,
    calls: AtomicUsize,
}

impl FileSystemFixture for UnavailableWriteFixture {
    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }
    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }
    fn prepare_write(
        &self,
        _: WriteScenario,
        _: &str,
        _: &[u8],
    ) -> FixtureResult<FixturePreparation<WriteFixtureCase>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(FixturePreparation::NotApplicable {
            reason: "fixture opted out".to_owned(),
        })
    }
}

#[test]
fn test_default_creation_uses_create_new() {
    let fixture = MemoryFixture::new();
    let prepared = fixture
        .prepare_write(WriteScenario::Create, "fresh", b"bytes")
        .expect("prepare creation");
    let FixturePreparation::Ready(case) = prepared else {
        panic!("fresh creation must have a request");
    };
    assert_eq!(case.options().disposition(), WriteDisposition::CreateNew);
    assert_eq!(case.bytes(), b"bytes");
    assert!(
        fixture.is_empty(),
        "request preparation must not publish through the tested facade"
    );
}

#[test]
fn test_guaranteed_write_cannot_skip_positive_evidence() {
    let fixture = UnavailableWriteFixture {
        inner: MemoryFixture::new(),
        calls: AtomicUsize::new(0),
    };
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Write);
    assert_eq!(
        fixture.calls.load(Ordering::Relaxed),
        7,
        "each declared write scenario prepares independently"
    );
    assert!(!run.requirements_satisfied());
    let basic = run
        .report()
        .checks()
        .iter()
        .find(|check| check.id() == ContractCheckId::WriteBasic)
        .expect("basic registered");
    assert!(matches!(basic.outcome(), ContractCheckOutcome::Unverified { .. }));
    let append = run
        .report()
        .checks()
        .iter()
        .find(|check| check.id() == ContractCheckId::AppendBasic)
        .expect("append registered");
    assert!(matches!(append.outcome(), ContractCheckOutcome::Unverified { .. }));
    for id in [
        ContractCheckId::WriteCreateConflict,
        ContractCheckId::WriteReplace,
        ContractCheckId::WriteAbort,
        ContractCheckId::WriteAtomicReplaceExisting,
        ContractCheckId::WriteDurable,
    ] {
        let check = run
            .report()
            .checks()
            .iter()
            .find(|check| check.id() == id)
            .expect("guarantee registered");
        assert!(matches!(check.outcome(), ContractCheckOutcome::Unverified { .. }));
    }
}

/// A fixture path error remains an ordinary typed error at the public runner.
struct FailingPathFixture(MemoryFixture);

impl FileSystemFixture for FailingPathFixture {
    fn file_system(&self) -> &FileSystem {
        self.0.file_system()
    }
    fn path(&self, _: &str) -> FixtureResult<Path> {
        Err(testkit::FixtureError::with_source(
            "path preparation failed",
            std::io::Error::other("native path failure"),
        ))
    }
    fn teardown(&self) -> FixtureResult<()> {
        self.0.teardown()
    }
}

#[test]
fn test_fixture_error_is_preserved_without_assertion_panic() {
    let fixture = FailingPathFixture(MemoryFixture::new());
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::ErrorContext);
    assert_eq!(run.failures().len(), 1);
    assert_eq!(run.failures()[0].check(), Some(ContractCheckId::ErrorContext));
    assert!(std::error::Error::source(&run.failures()[0]).is_some());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::Failed { .. }
    ));
}
