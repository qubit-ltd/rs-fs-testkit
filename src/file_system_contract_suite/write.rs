// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Implements writer and publication contracts.

use qubit_fs::metadata::FileSystemLimit;

use super::*;
use crate::FixtureCase;
use crate::internal::limit_probe_plan::MAX_PROBE_BYTES;
use crate::internal::limit_probe_plan::finite_probe;

impl<'a> FileSystemContractSuite<'a> {
    /// Checks writer behavior.
    pub fn assert_write(&mut self) {
        self.context.begin("write");
        if !self.capable(FileSystemCapability::Write) {
            let path = self.path("write-unavailable");
            let error = self
                .fixture
                .file_system()
                .open_writer(&path, Default::default())
                .expect_err("writer contract: unadvertised writer open succeeded");
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::OpenWriter,
                &path,
                None,
            );
            self.context.record_check(
                "write/basic",
                Some(FileSystemCapability::Write),
                ContractCheckOutcome::RejectedAsExpected,
            );
            for (id, capability) in [
                ("write/limit", FileSystemCapability::Write),
                ("write/if-absent", FileSystemCapability::ConditionalWrite),
                ("write/if-match", FileSystemCapability::ConditionalWrite),
                ("write/atomic-replace-existing", FileSystemCapability::AtomicReplace),
                ("write/durable", FileSystemCapability::DurableWrite),
            ] {
                self.context.record_check(
                    id,
                    Some(capability),
                    ContractCheckOutcome::NotApplicable {
                        reason: "Write capability is unavailable".to_owned(),
                    },
                );
            }
            return;
        }
        let limit = self.context.properties().limits().max_write_bytes();
        let initial = bounded_payload(limit, b"written", b'x');
        let path = self.path("write-file");
        self.context.record_created(path.clone());
        let outcome = self
            .fixture
            .file_system()
            .write_all(&path, &initial, WriteOptions::default())
            .expect("writer contract: write failed");
        if let Some(bytes_written) = outcome.bytes_written() {
            assert_eq!(bytes_written, initial.len() as u64);
        }
        self.assert_bytes(&path, &initial, "write/basic: writer contract: write was not published");
        self.context.record_check(
            "write/basic",
            Some(FileSystemCapability::Write),
            ContractCheckOutcome::Passed,
        );
        self.record_write_limit(limit);
        self.assert_write_options(&path, &initial);
    }

    /// Verifies the finite write boundary without exceeding the probe budget.
    fn record_write_limit(&mut self, limit: FileSystemLimit) {
        let outcome = match finite_probe(limit, MAX_PROBE_BYTES) {
            Some((maximum, over)) if maximum > 0 => {
                let maximum_bytes = usize::try_from(maximum).expect("write contract: bounded probe must fit usize");
                let at_limit = vec![b'x'; maximum_bytes];
                let boundary_path = self.path("write-limit-boundary");
                self.context.record_created(boundary_path.clone());
                let boundary = self
                    .fixture
                    .file_system()
                    .write_all(&boundary_path, &at_limit, WriteOptions::default())
                    .expect("write/limit: request at declared boundary failed");
                if let Some(bytes_written) = boundary.bytes_written() {
                    assert_eq!(bytes_written, maximum, "write/limit: boundary byte count mismatch");
                }
                self.assert_bytes(
                    &boundary_path,
                    &at_limit,
                    "write/limit: boundary request was not published",
                );
                let over_bytes =
                    vec![b'x'; usize::try_from(over).expect("write contract: bounded successor must fit usize")];
                let path = self.path("write-limit");
                self.context.record_created(path.clone());
                let failure = self
                    .fixture
                    .file_system()
                    .write_all(&path, &over_bytes, WriteOptions::default())
                    .expect_err("write/limit: declared write limit was ignored");
                assert_eq!(
                    failure.error().kind(),
                    FsErrorKind::ResourceLimitExceeded,
                    "write/limit: boundary request returned the wrong error"
                );
                ContractCheckOutcome::Passed
            }
            Some(_) => ContractCheckOutcome::SkippedOptional {
                reason: "write boundary exceeds the bounded probe budget".to_owned(),
            },
            None => ContractCheckOutcome::SkippedOptional {
                reason: "write limit is unknown, inapplicable, or unbounded".to_owned(),
            },
        };
        self.context
            .record_check("write/limit", Some(FileSystemCapability::Write), outcome);
    }

    /// Checks write dispositions, abort behavior, and conditional writes.
    pub fn assert_write_options(&mut self, existing: &Path, initial: &[u8]) {
        let limit = self.context.properties().limits().max_write_bytes();
        let create_new = WriteOptions::default().with_disposition(WriteDisposition::CreateNew);
        let unexpected = bounded_payload(limit, b"unexpected", b'u');
        let failure = self
            .fixture
            .file_system()
            .write_all(existing, &unexpected, create_new)
            .expect_err("writer contract: create-new replaced an existing target");
        self.assert_error(
            failure.error(),
            FsErrorKind::AlreadyExists,
            failure.error().operation(),
            existing,
            None,
        );
        self.assert_bytes(existing, initial, "writer contract: failed create-new changed target");

        let replacement = bounded_payload(limit, b"replaced", b'r');
        self.fixture
            .file_system()
            .write_all(existing, &replacement, WriteOptions::default())
            .expect("writer contract: replacement failed");
        self.assert_bytes(existing, &replacement, "writer contract: replacement bytes mismatch");

        let aborted_path = self.path("write-aborted");
        self.context.record_created(aborted_path.clone());
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&aborted_path, WriteOptions::default())
            .expect("writer contract: abort writer open failed");
        let aborted = bounded_payload(limit, b"aborted", b'a');
        Output::write_fully(&mut writer, &aborted).expect("writer contract: abort writer rejected bytes");
        let _ = writer.abort().expect("writer contract: abort failed");
        assert!(
            !self
                .fixture
                .file_system()
                .exists(&aborted_path)
                .expect("writer contract: aborted path observation failed")
        );

        let conditional_path = self.path("write-conditional");
        let conditional = WriteOptions::default().with_precondition(WritePrecondition::IfAbsent);
        let if_absent_support = if self.capable(FileSystemCapability::ConditionalWrite) {
            self.fixture
                .case_support(FixtureCase::WriteIfAbsent)
                .expect("conditional-write contract: If-Absent case query failed")
        } else {
            FixtureSupport::Unsupported
        };
        if !self.capable(FileSystemCapability::ConditionalWrite) {
            let error = self
                .fixture
                .file_system()
                .open_writer(&conditional_path, conditional)
                .expect_err("writer contract: unadvertised conditional write succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::OpenWriter,
                FileSystemCapability::ConditionalWrite,
                "conditional-write contract",
            );
            self.context.record_check(
                "write/if-absent",
                Some(FileSystemCapability::ConditionalWrite),
                ContractCheckOutcome::RejectedAsExpected,
            );
        } else if matches!(if_absent_support, FixtureSupport::Unsupported) {
            self.context.record_check(
                "write/if-absent",
                Some(FileSystemCapability::ConditionalWrite),
                ContractCheckOutcome::Unverified {
                    reason: "fixture cannot prepare If-Absent case".to_owned(),
                },
            );
        } else {
            let conditional_bytes = bounded_payload(limit, b"conditional", b'c');
            let retry_bytes = bounded_payload(limit, b"unexpected", b'u');
            self.context.record_created(conditional_path.clone());
            self.fixture
                .file_system()
                .write_all(&conditional_path, &conditional_bytes, conditional.clone())
                .expect("writer contract: advertised conditional write failed");
            let failure = self
                .fixture
                .file_system()
                .write_all(&conditional_path, &retry_bytes, conditional)
                .expect_err("writer contract: failed conditional write unexpectedly succeeded");
            self.assert_error(
                failure.error(),
                FsErrorKind::PreconditionFailed,
                failure.error().operation(),
                &conditional_path,
                None,
            );
            self.assert_bytes(
                &conditional_path,
                &conditional_bytes,
                "writer contract: failed condition changed target bytes",
            );
            self.context.record_check(
                "write/if-absent",
                Some(FileSystemCapability::ConditionalWrite),
                ContractCheckOutcome::Passed,
            );
        }

        let if_match_path = self.path("write-if-match");
        let if_match_support = if self.capable(FileSystemCapability::ConditionalWrite) {
            self.fixture
                .case_support(FixtureCase::WriteIfMatch)
                .expect("conditional-write contract: If-Match case query failed")
        } else {
            FixtureSupport::Unsupported
        };
        if !self.capable(FileSystemCapability::ConditionalWrite) {
            let error = self
                .fixture
                .file_system()
                .open_writer(
                    &if_match_path,
                    WriteOptions::default()
                        .with_precondition(WritePrecondition::IfMatch(ResourceVersion::new("missing-capability"))),
                )
                .expect_err("writer contract: unadvertised If-Match write succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::OpenWriter,
                FileSystemCapability::ConditionalWrite,
                "conditional-write contract",
            );
            self.context.record_check(
                "write/if-match",
                Some(FileSystemCapability::ConditionalWrite),
                ContractCheckOutcome::RejectedAsExpected,
            );
        } else if matches!(if_match_support, FixtureSupport::Unsupported) {
            self.context.record_check(
                "write/if-match",
                Some(FileSystemCapability::ConditionalWrite),
                ContractCheckOutcome::Unverified {
                    reason: "fixture cannot prepare If-Match case".to_owned(),
                },
            );
        } else {
            let if_match_path = self.required_seed("write-if-match", b"a", "conditional-write");
            let current = match self
                .fixture
                .resource_version(&if_match_path)
                .expect("write contract: current version observation failed")
            {
                FixtureSupport::Supported(version) => Some(version),
                FixtureSupport::Unsupported => {
                    self.context.record_check(
                        "write/if-match",
                        Some(FileSystemCapability::ConditionalWrite),
                        ContractCheckOutcome::Unverified {
                            reason: "fixture current version unavailable".to_owned(),
                        },
                    );
                    None
                }
            };
            if let Some(current) = current {
                let current_bytes = bounded_payload(limit, b"current", b'm');
                self.fixture
                    .file_system()
                    .write_all(
                        &if_match_path,
                        &current_bytes,
                        WriteOptions::default().with_precondition(WritePrecondition::IfMatch(current)),
                    )
                    .expect("writer contract: current If-Match write failed");
                let stale = self
                    .fixture
                    .stale_resource_version(&if_match_path)
                    .expect("write contract: stale version observation failed");
                if let FixtureSupport::Supported(stale) = stale {
                    let stale_bytes = bounded_payload(limit, b"stale", b's');
                    let failure = self
                        .fixture
                        .file_system()
                        .write_all(
                            &if_match_path,
                            &stale_bytes,
                            WriteOptions::default().with_precondition(WritePrecondition::IfMatch(stale)),
                        )
                        .expect_err("write/if-match: stale If-Match succeeded");
                    self.assert_error(
                        failure.error(),
                        FsErrorKind::PreconditionFailed,
                        failure.error().operation(),
                        &if_match_path,
                        None,
                    );
                    self.assert_bytes(
                        &if_match_path,
                        &current_bytes,
                        "write contract: stale If-Match changed bytes",
                    );
                    self.context.record_check(
                        "write/if-match",
                        Some(FileSystemCapability::ConditionalWrite),
                        ContractCheckOutcome::Passed,
                    );
                } else {
                    self.context.record_check(
                        "write/if-match",
                        Some(FileSystemCapability::ConditionalWrite),
                        ContractCheckOutcome::Unverified {
                            reason: "fixture stale version unavailable".to_owned(),
                        },
                    );
                }
            }
        }

        if self.capable(FileSystemCapability::AtomicReplace) {
            let atomic_path = self.required_seed("atomic-replace-existing", b"a", "atomic-replace");
            let replacement = bounded_payload(limit, b"b", b'b');
            let outcome = self
                .fixture
                .file_system()
                .write_all(
                    &atomic_path,
                    &replacement,
                    WriteOptions::default().with_atomicity(AtomicityRequirement::Required),
                )
                .expect("atomic-replace contract: required write failed");
            assert_eq!(
                outcome.atomicity(),
                AchievedAtomicity::Atomic,
                "atomic-replace contract: non-atomic outcome"
            );
            self.assert_bytes(
                &atomic_path,
                &replacement,
                "atomic-replace contract: old bytes retained",
            );
            self.context.record_check(
                "write/atomic-replace-existing",
                Some(FileSystemCapability::AtomicReplace),
                ContractCheckOutcome::Passed,
            );
        } else {
            let atomic_path = self.path("atomic-replace-unavailable");
            let error = self
                .fixture
                .file_system()
                .open_writer(
                    &atomic_path,
                    WriteOptions::default().with_atomicity(AtomicityRequirement::Required),
                )
                .expect_err("atomic-replace contract: unadvertised request succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::OpenWriter,
                FileSystemCapability::AtomicReplace,
                "atomic-replace contract",
            );
            self.context.record_check(
                "write/atomic-replace-existing",
                Some(FileSystemCapability::AtomicReplace),
                ContractCheckOutcome::RejectedAsExpected,
            );
        }
        if self.capable(FileSystemCapability::DurableWrite) {
            let durable_path = self.path("write-durable");
            let durable = bounded_payload(limit, b"durable", b'd');
            self.context.record_created(durable_path.clone());
            let outcome = self
                .fixture
                .file_system()
                .write_all(
                    &durable_path,
                    &durable,
                    WriteOptions::default().with_durability(DurabilityRequirement::Required),
                )
                .expect("write/durable: durable write failed");
            assert!(
                outcome.durable(),
                "write/durable: required durable write was not durable"
            );
            self.assert_bytes(&durable_path, &durable, "write/durable: durable bytes mismatch");
            self.context.record_check(
                "write/durable",
                Some(FileSystemCapability::DurableWrite),
                ContractCheckOutcome::Passed,
            );
        } else {
            let durable_path = self.path("durable-write-unavailable");
            let error = self
                .fixture
                .file_system()
                .open_writer(
                    &durable_path,
                    WriteOptions::default().with_durability(DurabilityRequirement::Required),
                )
                .expect_err("durable-write contract: unadvertised request succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::OpenWriter,
                FileSystemCapability::DurableWrite,
                "durable-write contract",
            );
            self.context.record_check(
                "write/durable",
                Some(FileSystemCapability::DurableWrite),
                ContractCheckOutcome::RejectedAsExpected,
            );
        }
    }

    /// Checks append writes.
    pub fn assert_append(&mut self) {
        self.context.begin("append");
        let path = self.path("append-target");
        let options = WriteOptions::default().with_disposition(WriteDisposition::Append);
        if !self.capable(FileSystemCapability::Append) {
            let error = self
                .fixture
                .file_system()
                .open_writer(&path, options)
                .expect_err("append contract: unadvertised append preflight succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::OpenWriter,
                FileSystemCapability::Append,
                "append contract",
            );
            self.context.record_check(
                "append/basic",
                Some(FileSystemCapability::Append),
                ContractCheckOutcome::RejectedAsExpected,
            );
            return;
        }
        let path = self.required_seed("append-target", b"before", "append");
        let append_bytes = bounded_payload(self.context.properties().limits().max_write_bytes(), b"-after", b'a');
        self.fixture
            .file_system()
            .write_all(&path, &append_bytes, options)
            .expect("append contract: append failed");
        let mut expected = b"before".to_vec();
        expected.extend_from_slice(&append_bytes);
        self.assert_bytes(&path, &expected, "append/basic: existing bytes were not retained");
        self.context.record_check(
            "append/basic",
            Some(FileSystemCapability::Append),
            ContractCheckOutcome::Passed,
        );
    }

    /// Checks required-atomic replacement against an existing target.
    pub fn assert_atomic_replace(&mut self) {
        self.context.begin("atomic_replace");
        if !self.capable(FileSystemCapability::Write) {
            self.context.record_check(
                "write/atomic-replace-existing",
                Some(FileSystemCapability::AtomicReplace),
                ContractCheckOutcome::NotApplicable {
                    reason: "Write capability is unavailable".to_owned(),
                },
            );
            return;
        }
        let path = self.required_seed("atomic-replace-target", b"a", "atomic-replace");
        let options = WriteOptions::default().with_atomicity(AtomicityRequirement::Required);
        if !self.capable(FileSystemCapability::AtomicReplace) {
            let error = self
                .fixture
                .file_system()
                .open_writer(&path, options)
                .expect_err("atomic-replace contract: unadvertised preflight succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::OpenWriter,
                FileSystemCapability::AtomicReplace,
                "atomic-replace contract",
            );
            self.context.record_check(
                "write/atomic-replace-existing",
                Some(FileSystemCapability::AtomicReplace),
                ContractCheckOutcome::RejectedAsExpected,
            );
            return;
        }
        let replacement = bounded_payload(self.context.properties().limits().max_write_bytes(), b"b", b'b');
        let outcome = self
            .fixture
            .file_system()
            .write_all(&path, &replacement, options)
            .expect("write/atomic-replace-existing: required-atomic write failed");
        assert_eq!(outcome.atomicity(), AchievedAtomicity::Atomic);
        self.assert_bytes(
            &path,
            &replacement,
            "write/atomic-replace-existing: replacement bytes mismatch",
        );
        self.context.record_check(
            "write/atomic-replace-existing",
            Some(FileSystemCapability::AtomicReplace),
            ContractCheckOutcome::Passed,
        );
    }
}

/// Selects a bounded write payload that fits the provider's advertised limit.
///
/// Unknown, inapplicable, and unbounded dimensions use the preferred payload.
/// A finite limit truncates the payload without ever allocating beyond the
/// preferred test vector; zero permits an empty publication probe.
fn bounded_payload(limit: FileSystemLimit, preferred: &[u8], fill: u8) -> Vec<u8> {
    let length = limit
        .maximum()
        .map_or(preferred.len() as u64, |maximum| maximum.min(preferred.len() as u64)) as usize;
    if length == preferred.len() {
        preferred.to_vec()
    } else {
        vec![fill; length]
    }
}
