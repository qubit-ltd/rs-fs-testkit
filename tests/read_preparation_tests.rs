// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Read scenarios retain precise ordinary failures and independent setup.

pub use ::qubit_fs_testkit;

mod common;
use qubit_fs::FileSystem;
use qubit_fs::metadata::ResourceVersion;
use qubit_fs::path::Path;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixturePreparation;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;
use qubit_fs_testkit::ReadScenario;

use self::common::MemoryFault;
use self::common::MemoryFixture;

struct StaleVersionUnavailableFixture {
    inner: MemoryFixture,
}

impl FileSystemFixture for StaleVersionUnavailableFixture {
    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }

    fn prepare_read(
        &self,
        scenario: ReadScenario,
        relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<FixturePreparation<Path>> {
        self.inner.prepare_read(scenario, relative, bytes)
    }

    fn resource_version(&self, path: &Path) -> FixtureResult<FixtureSupport<ResourceVersion>> {
        self.inner.resource_version(path)
    }

    fn stale_resource_version(&self, _path: &Path) -> FixtureResult<FixtureSupport<ResourceVersion>> {
        Ok(FixtureSupport::Unsupported)
    }
}

struct ReadScenarioNotApplicableFixture {
    inner: MemoryFixture,
}

impl FileSystemFixture for ReadScenarioNotApplicableFixture {
    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }

    fn prepare_read(
        &self,
        scenario: ReadScenario,
        relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<FixturePreparation<Path>> {
        if scenario == ReadScenario::Basic {
            Ok(FixturePreparation::NotApplicable {
                reason: "basic read setup cannot be independently created".to_owned(),
            })
        } else {
            self.inner.prepare_read(scenario, relative, bytes)
        }
    }
}
/// Deliberately rejected read requests must be distinguishable from reads.
#[test]
fn test_read_rejection_evidence_is_explicit() {
    let fixture = MemoryFixture::with_all_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Read);
    for id in [
        ContractCheckId::ReadIfMatchStale,
        ContractCheckId::ReadIfNoneMatchCurrent,
        ContractCheckId::ReadChecksumCorruption,
    ] {
        let check = run
            .report()
            .checks()
            .iter()
            .find(|check| check.id() == id)
            .expect("registered read check");
        assert!(
            matches!(check.outcome(), ContractCheckOutcome::RejectedAsExpected),
            "{id}: {:?}",
            check.outcome()
        );
    }
    assert!(run.requirements_satisfied());
}

/// Async read rejection has the same report semantics as synchronous read.
#[cfg(feature = "async")]
#[test]
fn test_async_read_rejection_evidence_is_explicit() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::new();
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(FileSystemContract::Read).await;
        for id in [
            ContractCheckId::ReadIfMatchStale,
            ContractCheckId::ReadIfNoneMatchCurrent,
            ContractCheckId::ReadChecksumCorruption,
        ] {
            let check = run
                .report()
                .checks()
                .iter()
                .find(|check| check.id() == id)
                .expect("registered read check");
            assert!(
                matches!(check.outcome(), ContractCheckOutcome::RejectedAsExpected),
                "{id}: {:?}",
                check.outcome()
            );
        }
        assert!(run.requirements_satisfied());
    });
}

/// Ignoring a stale condition is a failed check, not an unattributed panic.
#[test]
fn test_stale_read_failure_retains_check_identity() {
    let fixture = MemoryFixture::with_fault(MemoryFault::IgnoreReadIfMatch);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Read);
    assert_eq!(run.failures()[0].check(), Some(ContractCheckId::ReadIfMatchStale));
    assert!(
        run.report()
            .checks()
            .iter()
            .any(|check| check.id() == ContractCheckId::ReadIfMatchStale
                && matches!(check.outcome(), ContractCheckOutcome::Failed { .. }))
    );
    assert!(fixture.is_empty());
}

#[path = "common/selective_read_fixture.rs"]
mod selective_read_fixture;

/// A missing basic setup must not prevent an independently prepared range
/// check.
#[test]
fn test_read_checks_prepare_their_own_scenarios() {
    let fixture = selective_read_fixture::SelectiveReadFixture::new();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_contract(FileSystemContract::Read);
    assert!(!run.requirements_satisfied());
    assert!(run.failures().is_empty());
    assert!(
        run.report()
            .checks()
            .iter()
            .any(|check| check.id() == ContractCheckId::ReadBasic
                && matches!(check.outcome(), ContractCheckOutcome::Unverified { .. }))
    );
    assert!(run.report().checks().iter().any(
        |check| check.id() == ContractCheckId::ReadRange && matches!(check.outcome(), ContractCheckOutcome::Passed)
    ));
    assert!(run.cleanup().completed());
}

/// Both I/O drivers preserve the same stale-precondition failure identity.
#[cfg(feature = "async")]
#[test]
fn test_async_stale_read_failure_retains_check_identity() {
    use qubit_fs_testkit::AsyncFileSystemContractSuite;

    use self::common::AsyncMemoryFault;
    use self::common::AsyncMemoryFixture;
    use self::common::async_memory_file_system::run_controlled;
    let fixture = AsyncMemoryFixture::with_fault(AsyncMemoryFault::IgnoreReadIfMatch);
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        let run = suite.run_contract(FileSystemContract::Read).await;
        assert_eq!(run.failures()[0].check(), Some(ContractCheckId::ReadIfMatchStale));
        assert!(
            run.report()
                .checks()
                .iter()
                .any(|check| check.id() == ContractCheckId::ReadIfMatchStale
                    && matches!(check.outcome(), ContractCheckOutcome::Failed { .. }))
        );
        assert!(fixture.is_empty());
    });
}

#[test]
fn test_stale_read_version_without_independent_evidence_is_unverified() {
    let fixture = StaleVersionUnavailableFixture {
        inner: MemoryFixture::with_all_capabilities(),
    };
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::ReadIfMatchStale);
    assert!(
        run.failures().is_empty(),
        "missing version evidence is not a provider failure"
    );
    assert!(!run.requirements_satisfied());
    assert!(matches!(
        run.report()
            .checks()
            .iter()
            .find(|check| check.id() == ContractCheckId::ReadIfMatchStale)
            .expect("stale read check must be reported")
            .outcome(),
        ContractCheckOutcome::Unverified { reason } if reason.contains("stale version unavailable")
    ));
    assert!(run.cleanup().completed());
    assert!(fixture.inner.is_empty(), "version evidence setup must be cleaned");
}

#[test]
fn test_read_scenario_not_applicable_is_reported_as_unverified() {
    let fixture = ReadScenarioNotApplicableFixture {
        inner: MemoryFixture::with_all_capabilities(),
    };
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::ReadBasic);
    assert!(run.failures().is_empty(), "scenario applicability is a report outcome");
    assert!(!run.requirements_satisfied());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::Unverified { reason }
            if reason.contains("declared read capability requires scenario evidence")
    ));
    assert!(run.cleanup().completed());
    assert!(fixture.inner.is_empty(), "not-applicable setup must not leak resources");
}
