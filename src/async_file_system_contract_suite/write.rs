// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements writer and publication contracts.

use super::*;
use crate::internal::limit_probe_plan::{finite_probe, MAX_PROBE_BYTES};

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
            return;
        }
        let basic_bytes: Vec<u8> = if self
            .context
            .properties()
            .limits()
            .max_write_bytes()
            .maximum()
            .is_some_and(|maximum| maximum < 13)
        {
            b"a".to_vec()
        } else {
            b"async written".to_vec()
        };
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
                assert_eq!(
                    bytes, basic_bytes,
                    "write contract: bytes were not published"
                )
            }
            FixtureSupport::Unsupported => {
                panic!("write contract: Write capability requires fixture.read_file support")
            }
        }
        self.context.record_check(
            "write/basic",
            Some(FileSystemCapability::Write),
            ContractCheckOutcome::Passed,
        );
        if let Some((_, over)) = finite_probe(
            self.context.properties().limits().max_write_bytes(),
            MAX_PROBE_BYTES,
        ) {
            let limit_path = self.path("async-write-limit");
            let payload = vec![0_u8; over as usize];
            let failure = self
                .fixture
                .file_system()
                .write_all(&limit_path, &payload, WriteOptions::default())
                .await
                .expect_err("write contract: declared write limit was ignored");
            self.assert_error(
                failure.error(),
                FsErrorKind::ResourceLimitExceeded,
                failure.error().operation(),
                &limit_path,
            );
            self.context.record_check(
                "write/limit",
                Some(FileSystemCapability::Write),
                ContractCheckOutcome::Passed,
            );
        } else {
            self.context.record_check(
                "write/limit",
                Some(FileSystemCapability::Write),
                ContractCheckOutcome::SkippedOptional {
                    reason: "write limit is non-finite or outside probe budget".to_owned(),
                },
            );
        }
        self.assert_write_options(&path, &basic_bytes).await;
    }

    /// Checks asynchronous write dispositions, abort, and conditions.
    pub async fn assert_write_options(&mut self, existing: &Path, existing_bytes: &[u8]) {
        let small_payload = self
            .context
            .properties()
            .limits()
            .max_write_bytes()
            .maximum()
            .is_some_and(|maximum| maximum < 10);
        let unexpected = if small_payload {
            b"u".as_slice()
        } else {
            b"unexpected".as_slice()
        };
        let create_new = WriteOptions::default().with_disposition(WriteDisposition::CreateNew);
        let error = match self
            .fixture
            .file_system()
            .open_writer(existing, create_new)
            .await
        {
            Ok(mut writer) => {
                writer
                    .write_fully_async(unexpected)
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
            matches!(
                error.operation(),
                FsOperation::OpenWriter | FsOperation::CommitWriter
            ),
            "writer contract: create-new failed at an unrelated operation"
        );
        self.assert_error(
            &error,
            FsErrorKind::AlreadyExists,
            error.operation(),
            existing,
        );
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
        let replacement = if self
            .context
            .properties()
            .limits()
            .max_write_bytes()
            .maximum()
            .is_some_and(|maximum| maximum < 8)
        {
            b"b".as_slice()
        } else {
            b"replaced".as_slice()
        };
        writer
            .write_fully_async(replacement)
            .await
            .expect("writer contract: replacement writer rejected bytes");
        writer
            .commit_async()
            .await
            .expect("writer contract: replacement commit failed");
        self.assert_bytes(
            existing,
            replacement,
            "writer contract: replacement bytes mismatch",
        )
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
            .write_fully_async(if small_payload { b"a" } else { b"aborted" })
            .await
            .expect("writer contract: abort writer rejected bytes");
        let _ = writer
            .abort_async()
            .await
            .expect("writer contract: abort failed");
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
                    .write_fully_async(if small_payload { b"c" } else { b"conditional" })
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
                            .write_fully_async(unexpected)
                            .await
                            .expect("writer contract: conditional retry rejected bytes");
                        retry
                            .commit_async()
                            .await
                            .expect_err(
                                "writer contract: failed conditional write unexpectedly succeeded",
                            )
                            .into_error()
                    }
                    Err(error) => error,
                };
                assert!(
                    matches!(
                        error.operation(),
                        FsOperation::OpenWriter | FsOperation::CommitWriter
                    ),
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
                    if small_payload { b"c" } else { b"conditional" },
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

        let if_match_path = self
            .required_seed("async-write-if-match", b"if-match", "conditional-write")
            .await;
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
                        WriteOptions::default()
                            .with_precondition(WritePrecondition::IfMatch(current)),
                    )
                    .await
                    .expect("conditional-write contract: current writer open failed");
                writer
                    .write_fully_async(if small_payload {
                        b"m"
                    } else {
                        b"if-match-updated"
                    })
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
                let stale_options =
                    WriteOptions::default().with_precondition(WritePrecondition::IfMatch(stale));
                let error = match self
                    .fixture
                    .file_system()
                    .open_writer(&if_match_path, stale_options)
                    .await
                {
                    Ok(mut retry) => {
                        retry
                            .write_fully_async(unexpected)
                            .await
                            .expect("conditional-write contract: stale writer rejected bytes");
                        retry
                            .commit_async()
                            .await
                            .expect_err("conditional-write contract: stale commit succeeded")
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
                    if small_payload {
                        b"m"
                    } else {
                        b"if-match-updated"
                    },
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
                    WriteOptions::default().with_precondition(WritePrecondition::IfMatch(
                        ResourceVersion::new("unsupported-version"),
                    )),
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

        let atomic_path = self
            .required_seed("async-atomic-replace-existing", b"a", "atomic-replace")
            .await;
        if self.capable(FileSystemCapability::AtomicReplace) {
            let mut writer = self
                .fixture
                .file_system()
                .open_writer(
                    &atomic_path,
                    WriteOptions::default().with_atomicity(AtomicityRequirement::Required),
                )
                .await
                .expect("atomic-replace contract: writer open failed");
            writer
                .write_fully_async(b"b")
                .await
                .expect("atomic-replace contract: write failed");
            let outcome = writer
                .commit_async()
                .await
                .expect("atomic-replace contract: commit failed");
            assert_eq!(outcome.atomicity(), AchievedAtomicity::Atomic);
            self.assert_bytes(
                &atomic_path,
                b"b",
                "atomic-replace contract: old bytes retained",
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
            let outcome = self
                .fixture
                .file_system()
                .write_all(
                    &durable_path,
                    if small_payload { b"d" } else { b"durable" },
                    WriteOptions::default().with_durability(DurabilityRequirement::Required),
                )
                .await
                .expect("durable-write contract: required write failed");
            assert!(
                outcome.durable(),
                "durable-write contract: outcome was not durable"
            );
            self.assert_bytes(
                &durable_path,
                if small_payload { b"d" } else { b"durable" },
                "durable-write contract: bytes mismatch",
            )
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
        let path = self
            .required_seed("async-append-target", b"before", "append")
            .await;
        let append_bytes = if self
            .context
            .properties()
            .limits()
            .max_write_bytes()
            .maximum()
            .is_some_and(|maximum| maximum < 6)
        {
            b"a".as_slice()
        } else {
            b"-after".as_slice()
        };
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&path, options)
            .await
            .expect("append contract: writer open failed");
        writer
            .write_fully_async(append_bytes)
            .await
            .expect("append contract: write failed");
        writer
            .commit_async()
            .await
            .expect("append contract: commit failed");
        self.assert_bytes(
            &path,
            if append_bytes == b"a" {
                b"beforea".as_slice()
            } else {
                b"before-after".as_slice()
            },
            "append contract: existing bytes were not retained",
        )
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
            "atomic-replace contract: seed bytes were not published",
        )
        .await;
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&path, options)
            .await
            .expect("atomic-replace contract: writer open failed");
        writer
            .write_fully_async(b"b")
            .await
            .expect("atomic-replace contract: write failed");
        let outcome = writer
            .commit_async()
            .await
            .expect("atomic-replace contract: commit failed");
        assert_eq!(
            outcome.atomicity(),
            AchievedAtomicity::Atomic,
            "atomic-replace contract: non-atomic outcome"
        );
        self.assert_bytes(
            &path,
            b"b",
            "atomic-replace contract: old bytes were retained",
        )
        .await;
        self.context.record_check(
            "write/atomic-replace-existing",
            Some(FileSystemCapability::AtomicReplace),
            ContractCheckOutcome::Passed,
        );
    }
}
