// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements reader contracts.

use qubit_fs::metadata::FileSystemLimit;

use super::*;
use crate::internal::limit_probe_plan::MAX_PROBE_BYTES;

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
            for (id, capability) in [
                ("read/range", FileSystemCapability::RangeRead),
                ("read/range-limit", FileSystemCapability::RangeRead),
                ("read/if-match-current", FileSystemCapability::ConditionalRead),
                ("read/if-match-stale", FileSystemCapability::ConditionalRead),
                ("read/if-none-match-current", FileSystemCapability::ConditionalRead),
                ("read/if-none-match-stale", FileSystemCapability::ConditionalRead),
                ("read/checksum", FileSystemCapability::ChecksumValidation),
                ("read/checksum-corruption", FileSystemCapability::ChecksumValidation),
            ] {
                self.context.record_check(
                    id,
                    Some(capability),
                    ContractCheckOutcome::NotApplicable {
                        reason: "Read capability is unavailable".to_owned(),
                    },
                );
            }
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
                assert_eq!(actual, b"async bytes", "read/basic: seeded bytes mismatch");
                let error = self
                    .fixture
                    .file_system()
                    .read_all(&path, Default::default(), 4)
                    .await
                    .expect_err("read contract: caller byte limit was ignored");
                self.assert_error(&error, FsErrorKind::ResourceLimitExceeded, FsOperation::Read, &path);
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
        let range_limit = self.context.properties().limits().max_read_range_bytes();
        let range_length = range_limit.maximum().map_or(5, |limit| limit.min(5));
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
            let limit_outcome = match range_limit {
                FileSystemLimit::Maximum(maximum) if maximum < MAX_PROBE_BYTES => {
                    let over = maximum
                        .checked_add(1)
                        .expect("read/range-limit: range limit successor overflow");
                    self.fixture
                        .file_system()
                        .open_reader(path, ReadOptions::default().with_length(Some(maximum)))
                        .await
                        .expect("read/range-limit: boundary request was rejected");
                    let error = self
                        .fixture
                        .file_system()
                        .open_reader(path, ReadOptions::default().with_length(Some(over)))
                        .await
                        .expect_err("read/range-limit: declared range limit was ignored");
                    self.assert_error(
                        &error,
                        FsErrorKind::ResourceLimitExceeded,
                        FsOperation::OpenReader,
                        path,
                    );
                    ContractCheckOutcome::Passed
                }
                FileSystemLimit::Maximum(_) => ContractCheckOutcome::SkippedOptional {
                    reason: "range boundary exceeds the bounded probe budget".to_owned(),
                },
                FileSystemLimit::Unknown | FileSystemLimit::NotApplicable | FileSystemLimit::Unbounded => {
                    ContractCheckOutcome::SkippedOptional {
                        reason: "range limit is unknown, inapplicable, or unbounded".to_owned(),
                    }
                }
            };
            self.context
                .record_check("read/range-limit", Some(FileSystemCapability::RangeRead), limit_outcome);
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
                ContractCheckOutcome::NotApplicable {
                    reason: "RangeRead capability is unavailable".to_owned(),
                },
            );
        }

        let conditional_read = self.capable(FileSystemCapability::ConditionalRead);
        let if_match_support = if conditional_read {
            self.fixture
                .case_support(FixtureCase::ReadIfMatch)
                .expect("conditional-read contract: If-Match case query failed")
        } else {
            FixtureSupport::Unsupported
        };
        let if_none_match_support = if conditional_read {
            self.fixture
                .case_support(FixtureCase::ReadIfNoneMatch)
                .expect("conditional-read contract: If-None-Match case query failed")
        } else {
            FixtureSupport::Unsupported
        };
        let version_support = if conditional_read
            && (matches!(&if_match_support, FixtureSupport::Supported(_))
                || matches!(&if_none_match_support, FixtureSupport::Supported(_)))
        {
            self.fixture
                .resource_version(path)
                .await
                .expect("read contract: version observation failed")
        } else {
            FixtureSupport::Unsupported
        };
        let conditional = ReadOptions::default().with_if_match(Some(match &version_support {
            FixtureSupport::Supported(version) => version.clone(),
            FixtureSupport::Unsupported => ResourceVersion::new("contract-version"),
        }));
        if conditional_read {
            if matches!(&if_match_support, FixtureSupport::Unsupported)
                || !matches!(&version_support, FixtureSupport::Supported(_))
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
                            .expect_err("read/if-match-stale: stale If-Match succeeded");
                        self.assert_error(&error, FsErrorKind::PreconditionFailed, FsOperation::OpenReader, path);
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

        if conditional_read {
            if matches!(&if_none_match_support, FixtureSupport::Unsupported)
                || !matches!(&version_support, FixtureSupport::Supported(_))
            {
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
                let FixtureSupport::Supported(current) = &version_support else {
                    unreachable!("current version support checked above")
                };
                let current = self
                    .fixture
                    .file_system()
                    .open_reader(path, ReadOptions::default().with_if_none_match(Some(current.clone())))
                    .await
                    .expect_err("read/if-none-match-current: current If-None-Match succeeded");
                self.assert_error(&current, FsErrorKind::PreconditionFailed, FsOperation::OpenReader, path);
                self.context.record_check(
                    "read/if-none-match-current",
                    Some(FileSystemCapability::ConditionalRead),
                    ContractCheckOutcome::RejectedAsExpected,
                );
                let stale = self
                    .fixture
                    .stale_resource_version(path)
                    .await
                    .expect("conditional-read contract: stale version observation failed");
                match stale {
                    FixtureSupport::Supported(stale) => {
                        let bytes = self
                            .fixture
                            .file_system()
                            .read_all(path, ReadOptions::default().with_if_none_match(Some(stale)), 64)
                            .await
                            .expect("conditional-read contract: stale If-None-Match failed");
                        assert_eq!(bytes, b"async bytes");
                        self.context.record_check(
                            "read/if-none-match-stale",
                            Some(FileSystemCapability::ConditionalRead),
                            ContractCheckOutcome::Passed,
                        );
                    }
                    FixtureSupport::Unsupported => self.context.record_check(
                        "read/if-none-match-stale",
                        Some(FileSystemCapability::ConditionalRead),
                        ContractCheckOutcome::Unverified {
                            reason: "fixture cannot prepare stale If-None-Match case".to_owned(),
                        },
                    ),
                }
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
                        .expect_err("read/checksum: corrupted bytes were accepted");
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
            self.context.record_check(
                "read/checksum-corruption",
                Some(FileSystemCapability::ChecksumValidation),
                ContractCheckOutcome::NotApplicable {
                    reason: "ChecksumValidation capability is unavailable".to_owned(),
                },
            );
        }
    }
}
