// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements writer and publication contracts.

use qubit_fs::metadata::FileSystemLimit;

use super::*;
use crate::internal::limit_probe_plan::MAX_PROBE_BYTES;

impl<'a> AsyncFileSystemContractSuite<'a> {
    /// Checks asynchronous writer behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, publication, fixture observation, or
    /// structured error context violates the writer contract.
    pub async fn assert_write(&mut self) {
        self.context.begin("write");
        if !self.capable(FileSystemCapability::Write) {
            let path = self.path("async-write-unavailable");
            let error = self
                .fixture
                .file_system()
                .open_writer(&path, Default::default())
                .await
                .expect_err("write contract: unadvertised writer open succeeded");
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::OpenWriter,
                &path,
            );
            assert_eq!(
                error.required_capability(),
                Some(FileSystemCapability::Write),
                "write contract: missing required-capability context"
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
        let basic_bytes = bounded_payload(limit, b"async written", b'a');
        let path = self.path("async-write");
        self.context.record_created(path.clone());
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&path, Default::default())
            .await
            .expect("write contract: writer open failed");
        writer
            .write_fully_async(&basic_bytes)
            .await
            .expect("write contract: writer rejected bytes");
        let outcome = writer
            .commit_async()
            .await
            .expect("write contract: writer commit failed");
        if let Some(bytes_written) = outcome.bytes_written() {
            assert_eq!(
                bytes_written,
                basic_bytes.len() as u64,
                "writer contract: byte count mismatch"
            );
        }
        match self
            .fixture
            .read_file(&path)
            .await
            .expect("write contract: fixture observation failed")
        {
            FixtureSupport::Supported(bytes) => {
                assert_eq!(bytes, basic_bytes, "write contract: bytes were not published")
            }
            FixtureSupport::Unsupported => {
                panic!("write/basic: Write capability requires fixture.read_file support")
            }
        }
        self.context.record_check(
            "write/basic",
            Some(FileSystemCapability::Write),
            ContractCheckOutcome::Passed,
        );
        let write_limit = self.context.properties().limits().max_write_bytes();
        let limit_outcome = match write_limit {
            FileSystemLimit::Maximum(maximum) if maximum < MAX_PROBE_BYTES => {
                let over = maximum
                    .checked_add(1)
                    .expect("write/limit: write limit successor overflow");
                let at_path = self.path("async-write-limit-at");
                self.context.record_created(at_path.clone());
                let at_payload = vec![b'a'; usize::try_from(maximum).expect("write/limit-at: boundary must fit usize")];
                let mut operation = self
                    .fixture
                    .file_system()
                    .begin_write_all(at_path.clone(), &at_payload, WriteOptions::default())
                    .expect("write/limit-at: boundary request was rejected");
                operation
                    .execute()
                    .await
                    .expect("write/limit-at: boundary request was rejected");

                let over_path = self.path("async-write-limit-over");
                self.context.record_created(over_path.clone());
                let over_payload =
                    vec![b'o'; usize::try_from(over).expect("write/limit-over: successor must fit usize")];
                let failure = match self.fixture.file_system().begin_write_all(
                    over_path.clone(),
                    &over_payload,
                    WriteOptions::default(),
                ) {
                    Ok(mut operation) => operation
                        .execute()
                        .await
                        .expect_err("write/limit-over: declared write limit was ignored"),
                    Err(failure) => failure,
                };
                self.assert_error(
                    failure.error(),
                    FsErrorKind::ResourceLimitExceeded,
                    failure.error().operation(),
                    &over_path,
                );
                ContractCheckOutcome::Passed
            }
            FileSystemLimit::Maximum(_) => ContractCheckOutcome::SkippedOptional {
                reason: "write boundary exceeds the bounded probe budget".to_owned(),
            },
            FileSystemLimit::Unknown | FileSystemLimit::NotApplicable | FileSystemLimit::Unbounded => {
                ContractCheckOutcome::SkippedOptional {
                    reason: "write limit is unknown, inapplicable, or unbounded".to_owned(),
                }
            }
        };
        self.context
            .record_check("write/limit", Some(FileSystemCapability::Write), limit_outcome);
        self.assert_write_options(&path, &basic_bytes).await;
    }

    /// Checks asynchronous write dispositions, abort, and conditions.
    pub async fn assert_write_options(&mut self, existing: &Path, existing_bytes: &[u8]) {
        let limit = self.context.properties().limits().max_write_bytes();
        let unexpected = bounded_payload(limit, b"unexpected", b'u');
        let create_new = WriteOptions::default().with_disposition(WriteDisposition::CreateNew);
        let error = match self.fixture.file_system().open_writer(existing, create_new).await {
            Ok(mut writer) => {
                writer
                    .write_fully_async(&unexpected)
                    .await
                    .expect("writer contract: create-new writer rejected bytes");
                writer
                    .commit_async()
                    .await
                    .expect_err("writer contract: create-new replaced an existing target")
                    .into_error()
            }
            Err(error) => error,
        };
        assert!(
            matches!(error.operation(), FsOperation::OpenWriter | FsOperation::CommitWriter),
            "writer contract: create-new failed at an unrelated operation"
        );
        self.assert_error(&error, FsErrorKind::AlreadyExists, error.operation(), existing);
        self.assert_bytes(
            existing,
            existing_bytes,
            "writer contract: failed create-new changed the target",
        )
        .await;

        let mut writer = self
            .fixture
            .file_system()
            .open_writer(existing, WriteOptions::default())
            .await
            .expect("writer contract: replacement writer open failed");
        let replacement = bounded_payload(limit, b"replaced", b'r');
        writer
            .write_fully_async(&replacement)
            .await
            .expect("writer contract: replacement writer rejected bytes");
        writer
            .commit_async()
            .await
            .expect("writer contract: replacement commit failed");
        self.assert_bytes(existing, &replacement, "writer contract: replacement bytes mismatch")
            .await;

        let aborted_path = self.path("async-write-aborted");
        self.context.record_created(aborted_path.clone());
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&aborted_path, WriteOptions::default())
            .await
            .expect("writer contract: abort writer open failed");
        writer
            .write_fully_async(&bounded_payload(limit, b"aborted", b'a'))
            .await
            .expect("writer contract: abort writer rejected bytes");
        let _ = writer.abort_async().await.expect("writer contract: abort failed");
        assert!(
            !self
                .fixture
                .file_system()
                .exists(&aborted_path)
                .await
                .expect("writer contract: aborted path observation failed"),
            "writer contract: abort published the target"
        );

        let conditional_path = self.path("async-write-conditional");
        let conditional = WriteOptions::default().with_precondition(WritePrecondition::IfAbsent);
        if self.capable(FileSystemCapability::ConditionalWrite) {
            let if_absent_supported = matches!(
                self.fixture
                    .case_support(FixtureCase::WriteIfAbsent)
                    .expect("conditional-write contract: fixture case query failed"),
                FixtureSupport::Supported(())
            );
            if !if_absent_supported {
                self.context.record_check(
                    "write/if-absent",
                    Some(FileSystemCapability::ConditionalWrite),
                    ContractCheckOutcome::Unverified {
                        reason: "fixture cannot prepare If-Absent case".to_owned(),
                    },
                );
            } else {
                self.context.record_created(conditional_path.clone());
                let mut writer = self
                    .fixture
                    .file_system()
                    .open_writer(&conditional_path, conditional.clone())
                    .await
                    .expect("writer contract: conditional writer open failed");
                writer
                    .write_fully_async(&bounded_payload(limit, b"conditional", b'c'))
                    .await
                    .expect("writer contract: conditional writer rejected bytes");
                writer
                    .commit_async()
                    .await
                    .expect("writer contract: conditional commit failed");

                let error = match self
                    .fixture
                    .file_system()
                    .open_writer(&conditional_path, conditional)
                    .await
                {
                    Ok(mut retry) => {
                        retry
                            .write_fully_async(&unexpected)
                            .await
                            .expect("writer contract: conditional retry rejected bytes");
                        retry
                            .commit_async()
                            .await
                            .expect_err("writer contract: failed conditional write unexpectedly succeeded")
                            .into_error()
                    }
                    Err(error) => error,
                };
                assert!(
                    matches!(error.operation(), FsOperation::OpenWriter | FsOperation::CommitWriter),
                    "writer contract: conditional write failed at an unrelated operation"
                );
                self.assert_error(
                    &error,
                    FsErrorKind::PreconditionFailed,
                    error.operation(),
                    &conditional_path,
                );
                self.assert_bytes(
                    &conditional_path,
                    &bounded_payload(limit, b"conditional", b'c'),
                    "writer contract: failed condition changed target bytes",
                )
                .await;
                self.context.record_check(
                    "write/if-absent",
                    Some(FileSystemCapability::ConditionalWrite),
                    ContractCheckOutcome::Passed,
                );
            }
        } else {
            let error = self
                .fixture
                .file_system()
                .open_writer(&conditional_path, conditional)
                .await
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
        }

        let if_match_path = self.path("async-write-if-match");
        if self.capable(FileSystemCapability::ConditionalWrite) {
            if matches!(
                self.fixture
                    .case_support(FixtureCase::WriteIfMatch)
                    .expect("conditional-write contract: fixture case query failed"),
                FixtureSupport::Unsupported
            ) {
                self.context.record_check(
                    "write/if-match",
                    Some(FileSystemCapability::ConditionalWrite),
                    ContractCheckOutcome::Unverified {
                        reason: "fixture cannot prepare If-Match case".to_owned(),
                    },
                );
            } else {
                let if_match_path = self
                    .required_seed("async-write-if-match", b"if-match", "conditional-write")
                    .await;
                let current = self
                    .fixture
                    .resource_version(&if_match_path)
                    .await
                    .expect("conditional-write contract: version observation failed");
                let FixtureSupport::Supported(current) = current else {
                    panic!("conditional-write contract: fixture lacks current version")
                };
                let mut writer = self
                    .fixture
                    .file_system()
                    .open_writer(
                        &if_match_path,
                        WriteOptions::default().with_precondition(WritePrecondition::IfMatch(current)),
                    )
                    .await
                    .expect("conditional-write contract: current writer open failed");
                writer
                    .write_fully_async(&bounded_payload(limit, b"if-match-updated", b'm'))
                    .await
                    .expect("conditional-write contract: current write failed");
                writer
                    .commit_async()
                    .await
                    .expect("conditional-write contract: current commit failed");
                let stale = self
                    .fixture
                    .stale_resource_version(&if_match_path)
                    .await
                    .expect("conditional-write contract: stale version observation failed");
                let FixtureSupport::Supported(stale) = stale else {
                    panic!("conditional-write contract: fixture lacks stale version")
                };
                let stale_options = WriteOptions::default().with_precondition(WritePrecondition::IfMatch(stale));
                let error = match self
                    .fixture
                    .file_system()
                    .open_writer(&if_match_path, stale_options)
                    .await
                {
                    Ok(mut retry) => {
                        retry
                            .write_fully_async(&unexpected)
                            .await
                            .expect("conditional-write contract: stale writer rejected bytes");
                        retry
                            .commit_async()
                            .await
                            .expect_err("write/if-match: stale commit succeeded")
                            .into_error()
                    }
                    Err(error) => error,
                };
                self.assert_error(
                    &error,
                    FsErrorKind::PreconditionFailed,
                    error.operation(),
                    &if_match_path,
                );
                self.assert_bytes(
                    &if_match_path,
                    &bounded_payload(limit, b"if-match-updated", b'm'),
                    "conditional-write contract: stale condition changed bytes",
                )
                .await;
                self.context.record_check(
                    "write/if-match",
                    Some(FileSystemCapability::ConditionalWrite),
                    ContractCheckOutcome::Passed,
                );
            }
        } else {
            let error = self
                .fixture
                .file_system()
                .open_writer(
                    &if_match_path,
                    WriteOptions::default()
                        .with_precondition(WritePrecondition::IfMatch(ResourceVersion::new("unsupported-version"))),
                )
                .await
                .expect_err("writer contract: unadvertised If-Match write succeeded");
            self.assert_requirement_error(
                &error,
                error.operation(),
                FileSystemCapability::ConditionalWrite,
                "conditional-write contract",
            );
            self.context.record_check(
                "write/if-match",
                Some(FileSystemCapability::ConditionalWrite),
                ContractCheckOutcome::RejectedAsExpected,
            );
        }

        if self.capable(FileSystemCapability::AtomicReplace) {
            let atomic_path = self
                .required_seed("async-atomic-replace-existing", b"a", "atomic-replace")
                .await;
            let mut writer = self
                .fixture
                .file_system()
                .open_writer(
                    &atomic_path,
                    WriteOptions::default().with_atomicity(AtomicityRequirement::Required),
                )
                .await
                .expect("write/atomic-replace-existing: writer open failed");
            writer
                .write_fully_async(&bounded_payload(limit, b"b", b'b'))
                .await
                .expect("write/atomic-replace-existing: write failed");
            let outcome = writer
                .commit_async()
                .await
                .expect("write/atomic-replace-existing: commit failed");
            assert_eq!(outcome.atomicity(), AchievedAtomicity::Atomic);
            self.assert_bytes(
                &atomic_path,
                &bounded_payload(limit, b"b", b'b'),
                "write/atomic-replace-existing: old bytes retained",
            )
            .await;
            self.context.record_check(
                "write/atomic-replace-existing",
                Some(FileSystemCapability::AtomicReplace),
                ContractCheckOutcome::Passed,
            );
        } else {
            self.context.record_check(
                "write/atomic-replace-existing",
                Some(FileSystemCapability::AtomicReplace),
                ContractCheckOutcome::RejectedAsExpected,
            );
        }

        let durable_path = self.path("async-durable-write");
        if self.capable(FileSystemCapability::DurableWrite) {
            self.context.record_created(durable_path.clone());
            let durable_bytes = bounded_payload(limit, b"durable", b'd');
            let mut operation = self
                .fixture
                .file_system()
                .begin_write_all(
                    durable_path.clone(),
                    &durable_bytes,
                    WriteOptions::default().with_durability(DurabilityRequirement::Required),
                )
                .expect("write/durable: preflight failed");
            let outcome = operation.execute().await.expect("write/durable: required write failed");
            assert!(outcome.durable(), "write/durable: outcome was not durable");
            self.assert_bytes(&durable_path, &durable_bytes, "write/durable: bytes mismatch")
                .await;
            self.context.record_check(
                "write/durable",
                Some(FileSystemCapability::DurableWrite),
                ContractCheckOutcome::Passed,
            );
        } else {
            self.context.record_check(
                "write/durable",
                Some(FileSystemCapability::DurableWrite),
                ContractCheckOutcome::RejectedAsExpected,
            );
        }
    }

    /// Checks asynchronous append writes when the provider advertises them.
    ///
    /// # Panics
    ///
    /// Panics when append preflight or publication violates the advertised
    /// capability, or fixture setup and observation fail.
    pub async fn assert_append(&mut self) {
        self.context.begin("append");
        let path = self.path("async-append-target");
        let options = WriteOptions::default().with_disposition(WriteDisposition::Append);
        if !self.capable(FileSystemCapability::Append) {
            let error = self
                .fixture
                .file_system()
                .open_writer(&path, options)
                .await
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
        let path = self.required_seed("async-append-target", b"before", "append").await;
        let limit = self.context.properties().limits().max_write_bytes();
        let append_bytes = bounded_payload(limit, b"-after", b'a');
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&path, options)
            .await
            .expect("append contract: writer open failed");
        writer
            .write_fully_async(&append_bytes)
            .await
            .expect("append contract: write failed");
        writer.commit_async().await.expect("append contract: commit failed");
        let mut expected = b"before".to_vec();
        expected.extend_from_slice(&append_bytes);
        self.assert_bytes(&path, &expected, "append/basic: existing bytes were not retained")
            .await;
        self.context.record_check(
            "append/basic",
            Some(FileSystemCapability::Append),
            ContractCheckOutcome::Passed,
        );
    }

    /// Checks asynchronous atomic replacement publication when advertised.
    ///
    /// # Panics
    ///
    /// Panics when atomic-replacement preflight, publication, or reported
    /// atomicity violates the advertised capability.
    pub async fn assert_atomic_replace(&mut self) {
        self.context.begin("atomic_replace");
        let options = WriteOptions::default().with_atomicity(AtomicityRequirement::Required);
        if !self.capable(FileSystemCapability::AtomicReplace) {
            let path = self.path("async-atomic-replace-target");
            let error = self
                .fixture
                .file_system()
                .open_writer(&path, options)
                .await
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
        let path = self
            .required_seed("async-atomic-replace-target", b"a", "atomic-replace")
            .await;
        self.assert_bytes(
            &path,
            b"a",
            "write/atomic-replace-existing: seed bytes were not published",
        )
        .await;
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&path, options)
            .await
            .expect("write/atomic-replace-existing: writer open failed");
        writer
            .write_fully_async(&bounded_payload(
                self.context.properties().limits().max_write_bytes(),
                b"b",
                b'b',
            ))
            .await
            .expect("write/atomic-replace-existing: write failed");
        let outcome = writer
            .commit_async()
            .await
            .expect("write/atomic-replace-existing: commit failed");
        assert_eq!(
            outcome.atomicity(),
            AchievedAtomicity::Atomic,
            "write/atomic-replace-existing: non-atomic outcome"
        );
        let replacement = bounded_payload(self.context.properties().limits().max_write_bytes(), b"b", b'b');
        self.assert_bytes(
            &path,
            &replacement,
            "write/atomic-replace-existing: old bytes were retained",
        )
        .await;
        self.context.record_check(
            "write/atomic-replace-existing",
            Some(FileSystemCapability::AtomicReplace),
            ContractCheckOutcome::Passed,
        );
    }
}

/// Selects a write payload that fits the declared provider limit.
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
