// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use std::cell::Cell;

use qubit_fs::FileSystem;
use qubit_fs::metadata::ResourceVersion;
use qubit_fs::path::Path;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::FileSystemContractSuite;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixturePreparation;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;
use qubit_fs_testkit::ReadScenario;

use self::common::MemoryFault;
use self::common::MemoryFixture;

extern crate qubit_fs_testkit;

struct ReadPreparationUnavailableFixture {
    inner: MemoryFixture,
}

impl FileSystemFixture for ReadPreparationUnavailableFixture {
    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }

    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn prepare_read(
        &self,
        _scenario: ReadScenario,
        _relative: &str,
        _bytes: &[u8],
    ) -> FixtureResult<FixturePreparation<Path>> {
        Ok(FixturePreparation::NotApplicable {
            reason: "read setup is deliberately unavailable".to_owned(),
        })
    }
}

struct CopySourceUnobservableFixture {
    inner: MemoryFixture,
}

impl FileSystemFixture for CopySourceUnobservableFixture {
    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }

    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn seed_file(&self, relative: &str, bytes: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        self.inner.seed_file(relative, bytes)
    }

    fn read_file(&self, _path: &Path) -> FixtureResult<FixtureSupport<Vec<u8>>> {
        Ok(FixtureSupport::Unsupported)
    }
}

struct DeleteMissingResultUnobservableFixture {
    inner: MemoryFixture,
    existence_observations: Cell<usize>,
}

impl FileSystemFixture for DeleteMissingResultUnobservableFixture {
    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }

    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn seed_file(&self, relative: &str, bytes: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        self.inner.seed_file(relative, bytes)
    }

    fn exists_out_of_band(&self, _path: &Path) -> FixtureResult<FixtureSupport<bool>> {
        let observation = self.existence_observations.get() + 1;
        self.existence_observations.set(observation);
        Ok(if observation == 1 {
            FixtureSupport::Supported(false)
        } else {
            FixtureSupport::Unsupported
        })
    }
}

struct ConditionalDeleteVersionUnavailableFixture {
    inner: MemoryFixture,
}

impl FileSystemFixture for ConditionalDeleteVersionUnavailableFixture {
    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }

    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }

    fn seed_file(&self, relative: &str, bytes: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        self.inner.seed_file(relative, bytes)
    }

    fn read_file(&self, path: &Path) -> FixtureResult<FixtureSupport<Vec<u8>>> {
        self.inner.read_file(path)
    }

    fn resource_version(&self, path: &Path) -> FixtureResult<FixtureSupport<ResourceVersion>> {
        self.inner.resource_version(path)
    }

    fn stale_resource_version(&self, _path: &Path) -> FixtureResult<FixtureSupport<ResourceVersion>> {
        Ok(FixtureSupport::Unsupported)
    }
}

#[test]
fn test_sync_suite_reports_unverified_when_independent_evidence_is_missing() {
    let read = ReadPreparationUnavailableFixture {
        inner: MemoryFixture::with_all_capabilities(),
    };
    let mut read_suite = FileSystemContractSuite::new(&read);
    let read_result = read_suite.run_check(ContractCheckId::ReadBasic);
    assert!(read_result.failures().is_empty());
    assert!(matches!(
        read_result.report().checks()[0].outcome(),
        ContractCheckOutcome::Unverified { .. }
    ));

    let copy = CopySourceUnobservableFixture {
        inner: MemoryFixture::with_all_capabilities(),
    };
    let mut copy_suite = FileSystemContractSuite::new(&copy);
    let copy_result = copy_suite.run_check(ContractCheckId::CopyBasic);
    assert!(copy_result.failures().is_empty());
    assert!(matches!(
        copy_result.report().checks()[0].outcome(),
        ContractCheckOutcome::Unverified { .. }
    ));

    let delete = DeleteMissingResultUnobservableFixture {
        inner: MemoryFixture::with_all_capabilities(),
        existence_observations: Cell::new(0),
    };
    let mut delete_suite = FileSystemContractSuite::new(&delete);
    let delete_result = delete_suite.run_check(ContractCheckId::DeleteMissingOk);
    assert!(delete_result.failures().is_empty());
    assert!(matches!(
        delete_result.report().checks()[0].outcome(),
        ContractCheckOutcome::Unverified { .. }
    ));
    assert_eq!(delete.existence_observations.get(), 2);

    let conditional_delete = ConditionalDeleteVersionUnavailableFixture {
        inner: MemoryFixture::with_all_capabilities(),
    };
    let mut conditional_suite = FileSystemContractSuite::new(&conditional_delete);
    let conditional_result = conditional_suite.run_check(ContractCheckId::DeleteIfMatch);
    assert!(conditional_result.failures().is_empty());
    assert!(matches!(
        conditional_result.report().checks()[0].outcome(),
        ContractCheckOutcome::Unverified { .. }
    ));
}

#[test]
fn test_sync_suite_attributes_publication_and_metadata_errors_to_selected_check() {
    for (id, fault, message) in [
        (
            ContractCheckId::AppendBasic,
            MemoryFault::WriteFails,
            "append publication failed",
        ),
        (
            ContractCheckId::WriteBasic,
            MemoryFault::WriteFails,
            "write/basic: write failed",
        ),
        (
            ContractCheckId::WriteReplace,
            MemoryFault::WriteFails,
            "replacement publication failed",
        ),
        (
            ContractCheckId::WriteIfAbsent,
            MemoryFault::WriteFails,
            "conditional write first publication failed",
        ),
        (
            ContractCheckId::WriteIfMatch,
            MemoryFault::WriteFails,
            "conditional write first publication failed",
        ),
        (
            ContractCheckId::DirectoryCreate,
            MemoryFault::CreateDirectoryFails,
            "directory creation failed",
        ),
        (
            ContractCheckId::CopyBasic,
            MemoryFault::CopyFails,
            "copy execution failed",
        ),
        (
            ContractCheckId::CopyFallbackOverwriteRejected,
            MemoryFault::CopyOverwriteFails,
            "copy conflict execution failed",
        ),
        (
            ContractCheckId::ReadRange,
            MemoryFault::ReadFailsOnEmptyRange,
            "empty range observation failed",
        ),
        (
            ContractCheckId::RepresentationEmpty,
            MemoryFault::CleanupStatError,
            "representation metadata observation failed",
        ),
        (
            ContractCheckId::StatFileKind,
            MemoryFault::CleanupStatError,
            "seeded file stat failed",
        ),
    ] {
        let fixture = MemoryFixture::with_fault_and_all_capabilities(fault);
        let mut suite = FileSystemContractSuite::new(&fixture);
        let run = suite.run_check(id);

        assert_eq!(
            run.failures().first().and_then(|failure| failure.check()),
            Some(id),
            "{id}: {:?}; report: {:?}",
            run.failures(),
            run.report().checks()
        );
        assert!(
            run.failures()
                .iter()
                .any(|failure| failure.to_string().contains(message)),
            "{id}: expected failure containing {message:?}, got {:?}",
            run.failures()
        );
    }
}

#[test]
fn test_sync_suite_public_helpers_seed_and_track_resources() {
    let fixture = MemoryFixture::with_all_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);

    let path = suite.path("support-path");
    assert!(path.as_str().ends_with("support-path"));
    assert!(matches!(
        suite.seed("support-seed", b"seeded"),
        FixtureSupport::Supported(_)
    ));
    let required = suite.required_seed("required-support-seed", b"required", "coverage probe");
    assert!(required.as_str().ends_with("required-support-seed"));

    suite.finish();
    assert!(
        fixture.is_empty(),
        "public suite helpers must register seeded paths for cleanup"
    );
}
