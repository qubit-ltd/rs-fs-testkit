// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements reader contracts.

use super::*;
use crate::internal::limit_probe_plan::{finite_probe, MAX_PROBE_BYTES};

impl<'a> AsyncFileSystemContractSuite<'a> {
    /// Checks asynchronous reader behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, seeded reads, byte limits, or
    /// structured error context violates the reader contract.
    pub async fn assert_read(&mut self) {
        self.context.begin("read");
        if !self.capable(FileSystemCapability::Read) {
            let path = self.path("async-read-unavailable");
            let error = self
                .fixture
                .file_system()
                .open_reader(&path, Default::default())
                .await
                .expect_err("read contract: unadvertised reader open succeeded");
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::OpenReader,
                &path,
            );
            assert_eq!(
                error.required_capability(),
                Some(FileSystemCapability::Read),
                "read contract: missing required-capability context"
            );
            self.context.record_check(
                "read/basic",
                Some(FileSystemCapability::Read),
                ContractCheckOutcome::RejectedAsExpected,
            );
            return;
        }
        let path = match self
            .fixture
            .seed_file("async-read", b"async bytes")
            .await
            .expect("read contract: fixture seed failed")
        {
            FixtureSupport::Supported(path) => {
                self.context.record_created(path.clone());
                let actual = self
                    .fixture
                    .file_system()
                    .read_all(&path, Default::default(), 64)
                    .await
                    .expect("read contract: facade could not read seeded bytes");
                assert_eq!(
                    actual, b"async bytes",
                    "read contract: seeded bytes mismatch"
                );
                let error = self
                    .fixture
                    .file_system()
                    .read_all(&path, Default::default(), 4)
                    .await
                    .expect_err("read contract: caller byte limit was ignored");
                self.assert_error(
                    &error,
                    FsErrorKind::ResourceLimitExceeded,
                    FsOperation::Read,
                    &path,
                );
                self.context.record_check(
                    "read/basic",
                    Some(FileSystemCapability::Read),
                    ContractCheckOutcome::Passed,
                );
                path
            }
            FixtureSupport::Unsupported => {
                panic!("read contract: advertised capability requires fixture.seed_file support")
            }
        };
        self.assert_read_options(&path).await;
    }

    /// Checks asynchronous range, conditional, and checksum read guarantees.
    pub async fn assert_read_options(&mut self, path: &Path) {
        let range_limit = self
            .context
            .properties()
            .limits()
            .max_read_range_bytes()
            .maximum();
        let range_length = range_limit.map_or(5, |limit| limit.min(5));
        let range = ReadOptions::default()
            .with_offset(Some(0))
            .with_length(Some(range_length));
        if self.capable(FileSystemCapability::RangeRead) {
            let bytes = self
                .fixture
                .file_system()
                .read_all(path, range, 64)
                .await
                .expect("read contract: advertised range read failed");
            assert_eq!(
                bytes,
                &b"async bytes"[..range_length as usize],
                "read contract: range mismatch"
            );
            self.context.record_check(
                "read/range",
                Some(FileSystemCapability::RangeRead),
                ContractCheckOutcome::Passed,
            );
            if let Some((_, over)) = finite_probe(
                self.context.properties().limits().max_read_range_bytes(),
                MAX_PROBE_BYTES,
            ) {
                let error = self
                    .fixture
                    .file_system()
                    .open_reader(path, ReadOptions::default().with_length(Some(over)))
                    .await
                    .expect_err("read contract: declared range limit was ignored");
                self.assert_error(
                    &error,
                    FsErrorKind::ResourceLimitExceeded,
                    FsOperation::OpenReader,
                    path,
                );
                self.context.record_check(
                    "read/range-limit",
                    Some(FileSystemCapability::RangeRead),
                    ContractCheckOutcome::Passed,
                );
            } else {
                self.context.record_check(
                    "read/range-limit",
                    Some(FileSystemCapability::RangeRead),
                    ContractCheckOutcome::SkippedOptional {
                        reason: "range limit is non-finite or outside probe budget".to_owned(),
                    },
                );
            }
        } else {
            let error = self
                .fixture
                .file_system()
                .open_reader(path, range)
                .await
                .expect_err("read contract: unadvertised range read succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::OpenReader,
                FileSystemCapability::RangeRead,
                "range-read contract",
            );
            self.context.record_check(
                "read/range",
                Some(FileSystemCapability::RangeRead),
                ContractCheckOutcome::RejectedAsExpected,
            );
            self.context.record_check(
                "read/range-limit",
                Some(FileSystemCapability::RangeRead),
                ContractCheckOutcome::RejectedAsExpected,
            );
        }

        let version_support = self
            .fixture
            .resource_version(path)
            .await
            .expect("read contract: version observation failed");
        let conditional = ReadOptions::default().with_if_match(Some(match &version_support {
            FixtureSupport::Supported(version) => version.clone(),
            FixtureSupport::Unsupported => ResourceVersion::new("contract-version"),
        }));
        if self.capable(FileSystemCapability::ConditionalRead) {
            if matches!(
                self.fixture
                    .case_support(FixtureCase::ReadIfMatch)
                    .expect("conditional-read contract: fixture case query failed"),
                FixtureSupport::Unsupported
            ) || !matches!(version_support, FixtureSupport::Supported(_))
            {
                self.context.record_check(
                    "read/if-match-current",
                    Some(FileSystemCapability::ConditionalRead),
                    ContractCheckOutcome::Unverified {
                        reason: "fixture cannot prepare current If-Match case".to_owned(),
                    },
                );
                self.context.record_check(
                    "read/if-match-stale",
                    Some(FileSystemCapability::ConditionalRead),
                    ContractCheckOutcome::Unverified {
                        reason: "fixture cannot prepare stale If-Match case".to_owned(),
                    },
                );
            } else {
                let bytes = self
                    .fixture
                    .file_system()
                    .read_all(path, conditional, 64)
                    .await
                    .expect("read contract: advertised conditional read failed");
                assert_eq!(bytes, b"async bytes");
                self.context.record_check(
                    "read/if-match-current",
                    Some(FileSystemCapability::ConditionalRead),
                    ContractCheckOutcome::Passed,
                );
                let stale = self
                    .fixture
                    .stale_resource_version(path)
                    .await
                    .expect("conditional-read contract: stale version observation failed");
                match stale {
                    FixtureSupport::Supported(version) => {
                        let error = self
                            .fixture
                            .file_system()
                            .open_reader(path, ReadOptions::default().with_if_match(Some(version)))
                            .await
                            .expect_err("conditional-read contract: stale If-Match succeeded");
                        self.assert_error(
                            &error,
                            FsErrorKind::PreconditionFailed,
                            FsOperation::OpenReader,
                            path,
                        );
                        self.context.record_check(
                            "read/if-match-stale",
                            Some(FileSystemCapability::ConditionalRead),
                            ContractCheckOutcome::RejectedAsExpected,
                        );
                    }
                    FixtureSupport::Unsupported => self.context.record_check(
                        "read/if-match-stale",
                        Some(FileSystemCapability::ConditionalRead),
                        ContractCheckOutcome::Unverified {
                            reason: "fixture cannot prepare stale If-Match case".to_owned(),
                        },
                    ),
                }
            }
        } else {
            let error = self
                .fixture
                .file_system()
                .open_reader(path, conditional)
                .await
                .expect_err("read contract: unadvertised conditional read succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::OpenReader,
                FileSystemCapability::ConditionalRead,
                "conditional-read contract",
            );
            self.context.record_check(
                "read/if-match-current",
                Some(FileSystemCapability::ConditionalRead),
                ContractCheckOutcome::RejectedAsExpected,
            );
            self.context.record_check(
                "read/if-match-stale",
                Some(FileSystemCapability::ConditionalRead),
                ContractCheckOutcome::RejectedAsExpected,
            );
        }

        let none_match_current =
            ReadOptions::default().with_if_none_match(Some(ResourceVersion::new("v1")));
        let none_match_stale =
            ReadOptions::default().with_if_none_match(Some(ResourceVersion::new("stale-v0")));
        if self.capable(FileSystemCapability::ConditionalRead) {
            if matches!(
                self.fixture
                    .case_support(FixtureCase::ReadIfNoneMatch)
                    .expect("conditional-read contract: fixture case query failed"),
                FixtureSupport::Unsupported
            ) {
                for id in ["read/if-none-match-current", "read/if-none-match-stale"] {
                    self.context.record_check(
                        id,
                        Some(FileSystemCapability::ConditionalRead),
                        ContractCheckOutcome::Unverified {
                            reason: "fixture cannot prepare If-None-Match cases".to_owned(),
                        },
                    );
                }
            } else {
                let current = self
                    .fixture
                    .file_system()
                    .open_reader(path, none_match_current)
                    .await
                    .expect_err("conditional-read contract: current If-None-Match succeeded");
                self.assert_error(
                    &current,
                    FsErrorKind::PreconditionFailed,
                    FsOperation::OpenReader,
                    path,
                );
                self.context.record_check(
                    "read/if-none-match-current",
                    Some(FileSystemCapability::ConditionalRead),
                    ContractCheckOutcome::RejectedAsExpected,
                );
                let stale = self
                    .fixture
                    .file_system()
                    .read_all(path, none_match_stale, 64)
                    .await
                    .expect("conditional-read contract: stale If-None-Match failed");
                assert_eq!(stale, b"async bytes");
                self.context.record_check(
                    "read/if-none-match-stale",
                    Some(FileSystemCapability::ConditionalRead),
                    ContractCheckOutcome::Passed,
                );
            }
        } else {
            for id in ["read/if-none-match-current", "read/if-none-match-stale"] {
                self.context.record_check(
                    id,
                    Some(FileSystemCapability::ConditionalRead),
                    ContractCheckOutcome::RejectedAsExpected,
                );
            }
        }

        let checksummed = ReadOptions::default().with_checksum(ChecksumPolicy::Required);
        if self.capable(FileSystemCapability::ChecksumValidation) {
            let bytes = self
                .fixture
                .file_system()
                .read_all(path, checksummed, 64)
                .await
                .expect("read contract: advertised checksum validation failed");
            assert_eq!(bytes, b"async bytes");
            self.context.record_check(
                "read/checksum",
                Some(FileSystemCapability::ChecksumValidation),
                ContractCheckOutcome::Passed,
            );
            let failure_relative = self.context.relative_name("async-checksum-failure");
            match self
                .fixture
                .checksum_failure_case(&failure_relative)
                .await
                .expect("checksum-read contract: failure case setup failed")
            {
                FixtureSupport::Supported(failure_path) => {
                    self.context.record_created(failure_path.clone());
                    let error = self
                        .fixture
                        .file_system()
                        .read_all(
                            &failure_path,
                            ReadOptions::default().with_checksum(ChecksumPolicy::Required),
                            64,
                        )
                        .await
                        .expect_err("checksum-read contract: corrupted bytes were accepted");
                    self.assert_error(
                        &error,
                        FsErrorKind::DataCorruption,
                        FsOperation::OpenReader,
                        &failure_path,
                    );
                    self.context.record_check(
                        "read/checksum-corruption",
                        Some(FileSystemCapability::ChecksumValidation),
                        ContractCheckOutcome::Passed,
                    );
                }
                FixtureSupport::Unsupported => self.context.record_check(
                    "read/checksum-corruption",
                    Some(FileSystemCapability::ChecksumValidation),
                    ContractCheckOutcome::SkippedOptional {
                        reason: "fixture has no independent checksum corruption probe".to_owned(),
                    },
                ),
            }
        } else {
            let error = self
                .fixture
                .file_system()
                .open_reader(path, checksummed)
                .await
                .expect_err("read contract: unadvertised checksum validation succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::OpenReader,
                FileSystemCapability::ChecksumValidation,
                "checksum-read contract",
            );
            self.context.record_check(
                "read/checksum",
                Some(FileSystemCapability::ChecksumValidation),
                ContractCheckOutcome::RejectedAsExpected,
            );
        }
    }
}
