// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Namespace and raw-root checks must be independently selectable contracts.

mod common;
use std::error::Error;

#[cfg(feature = "async")]
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::ContractCheckId;
use qubit_fs_testkit::ContractCheckOutcome;
use qubit_fs_testkit::ContractRun;
use qubit_fs_testkit::FileSystemContractSuite;
#[path = "support/flat_listing.rs"]
mod namespace_support;
use namespace_support::Fault;
use namespace_support::FlatFixture;

/// Hierarchical providers reject flat namespace requests with explicit
/// evidence.
#[test]
fn hierarchical_namespace_is_rejected_as_expected() {
    let fixture = common::MemoryFixture::with_all_capabilities();
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::ListNamespace);
    run.assert_satisfied();
    assert_eq!(ContractCheckId::ListNamespace.as_str(), "list/namespace");
    assert_eq!(ContractCheckId::ListRawRootPrefix.as_str(), "list/raw-root-prefix");
}

/// Complete namespace snapshots include keys that predate the selected check.
#[test]
fn flat_checks_accept_complete_results_and_reject_defects() {
    for (id, fault, satisfied) in [
        (ContractCheckId::ListNamespace, Fault::None, true),
        (ContractCheckId::ListRawRootPrefix, Fault::None, true),
        (ContractCheckId::ListLiteralPrefix, Fault::None, true),
        (ContractCheckId::ListNamespace, Fault::OmitExisting, false),
        (ContractCheckId::ListNamespace, Fault::Duplicate, false),
        (ContractCheckId::ListLiteralPrefix, Fault::IgnoreFilter, false),
        (ContractCheckId::ListRawRootPrefix, Fault::ComponentPrefix, false),
    ] {
        let fixture = FlatFixture::new(fault, true);
        let mut suite = FileSystemContractSuite::new(&fixture);
        assert_eq!(suite.run_check(id).requirements_satisfied(), satisfied, "{id}");
        #[cfg(feature = "async")]
        common::async_memory_file_system::run_controlled(async {
            let fixture = FlatFixture::new(fault, true);
            let mut suite = AsyncFileSystemContractSuite::new(&fixture);
            assert_eq!(
                suite.run_check(id).await.requirements_satisfied(),
                satisfied,
                "async {id}"
            );
        });
    }
}

/// Missing independent evidence must remain unverified instead of passing.
#[test]
fn absent_snapshot_cannot_prove_namespace_completeness() {
    let fixture = FlatFixture::new(Fault::None, false);
    let mut suite = FileSystemContractSuite::new(&fixture);
    let run = suite.run_check(ContractCheckId::ListNamespace);
    assert!(!run.requirements_satisfied());
    assert!(matches!(
        run.report().checks()[0].outcome(),
        ContractCheckOutcome::Unverified { .. }
    ));
}

/// Preparation and provider failures retain their check identity and cause.
#[test]
fn flat_check_failures_preserve_diagnostics() {
    for (id, fault, message) in [
        (
            ContractCheckId::ListRawRootPrefix,
            Fault::PathFailure,
            "raw prefix preparation failed",
        ),
        (
            ContractCheckId::ListNamespace,
            Fault::SeedFailure,
            "flat listing seed failed",
        ),
        (
            ContractCheckId::ListNamespace,
            Fault::SnapshotFailure,
            "namespace snapshot failed",
        ),
        (
            ContractCheckId::ListNamespace,
            Fault::OpenFailure,
            "flat listing open failed",
        ),
        (
            ContractCheckId::ListNamespace,
            Fault::StreamFailure,
            "flat listing stream failed",
        ),
    ] {
        let verify = |run: &ContractRun| {
            assert!(!run.requirements_satisfied());
            assert_eq!(run.failures().len(), 1);
            let failure = &run.failures()[0];
            assert_eq!(failure.check(), Some(id));
            assert_eq!(failure.message(), message);
            assert!(failure.source().is_some());
            assert!(run.cleanup().completed());
        };
        let fixture = FlatFixture::new(fault, true);
        verify(FileSystemContractSuite::new(&fixture).run_check(id));
        #[cfg(feature = "async")]
        common::async_memory_file_system::run_controlled(async {
            let fixture = FlatFixture::new(fault, true);
            verify(AsyncFileSystemContractSuite::new(&fixture).run_check(id).await);
        });
    }
}

/// Unsupported preparation differs from a snapshot that contradicts known
/// seeds.
#[test]
fn flat_check_missing_evidence_never_passes() {
    for (fault, snapshot, unverified) in [
        (Fault::SeedUnsupported, true, true),
        (Fault::None, false, true),
        (Fault::SnapshotIncomplete, true, false),
    ] {
        let verify = |run: &ContractRun| {
            assert!(!run.requirements_satisfied());
            if unverified {
                assert!(run.failures().is_empty());
                assert!(matches!(
                    run.report().checks()[0].outcome(),
                    ContractCheckOutcome::Unverified { .. }
                ));
            } else {
                assert_eq!(
                    run.failures()[0].message(),
                    "list/namespace: namespace snapshot omitted seeded paths"
                );
            }
        };
        let fixture = FlatFixture::new(fault, snapshot);
        verify(FileSystemContractSuite::new(&fixture).run_check(ContractCheckId::ListNamespace));
        #[cfg(feature = "async")]
        common::async_memory_file_system::run_controlled(async {
            let fixture = FlatFixture::new(fault, snapshot);
            verify(
                AsyncFileSystemContractSuite::new(&fixture)
                    .run_check(ContractCheckId::ListNamespace)
                    .await,
            );
        });
    }
}
