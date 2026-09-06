// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Implements reader contracts.

use super::*;
use crate::FixtureCase;
use crate::internal::limit_probe_plan::MAX_PROBE_BYTES;

impl<'a> FileSystemContractSuite<'a> {
    /// Checks reader behavior.
    pub fn assert_read(&mut self) {
        self.context.begin("read");
        let file_system = self.fixture.file_system();
        if !self.capable(FileSystemCapability::Read) {
            let path = self.path("read-unavailable");
            let error = file_system
                .open_reader(&path, Default::default())
                .expect_err("read contract: unadvertised reader open succeeded");
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::OpenReader,
                &path,
                None,
            );
            assert_eq!(
                error.required_capability(),
                Some(FileSystemCapability::Read)
            );
            self.context.record_check(
                "read/basic",
                Some(FileSystemCapability::Read),
                ContractCheckOutcome::RejectedAsExpected,
            );
            for id in [
                "read/range",
                "read/range-limit",
                "read/if-match-current",
                "read/if-match-stale",
                "read/if-none-match-current",
                "read/if-none-match-stale",
                "read/checksum",
                "read/checksum-corruption",
            ] {
                self.context.record_check(
                    id,
                    None,
                    ContractCheckOutcome::NotApplicable {
                        reason: "Read capability is unavailable".to_owned(),
                    },
                );
            }
            return;
        }
        let path = self.required_seed("read-file", b"read contract bytes", "read");
        let bytes = file_system
            .read_all(&path, Default::default(), 64)
            .expect("read contract: facade could not read seeded bytes");
        assert_eq!(
            bytes, b"read contract bytes",
            "read contract: bytes mismatch"
        );
        let error = file_system
            .read_all(&path, Default::default(), 4)
            .expect_err("read contract: caller byte limit was ignored");
        self.assert_error(
            &error,
            FsErrorKind::ResourceLimitExceeded,
            FsOperation::Read,
            &path,
            None,
        );
        self.context.record_check(
            "read/basic",
            Some(FileSystemCapability::Read),
            ContractCheckOutcome::Passed,
        );
        self.assert_read_options(&path);
    }

    /// Checks range, conditional, and checksum read guarantees.
    pub fn assert_read_options(&mut self, path: &Path) {
        const CONTENT: &[u8] = b"read contract bytes";
        let range_limit = self
            .context
            .properties()
            .limits()
            .max_read_range_bytes();
        let range_length = range_limit
            .maximum()
            .map_or(8, |maximum| maximum.min(8));
        let range_offset = if range_length >= 8 { 5 } else { 0 };
        let range = ReadOptions::default()
            .with_offset(Some(range_offset))
            .with_length(Some(range_length));
        if self.capable(FileSystemCapability::RangeRead) {
            let bytes = self
                .fixture
                .file_system()
                .read_all(path, range, 64)
                .expect("read contract: advertised range read failed");
            let end = range_offset
                .checked_add(range_length)
                .expect("read contract: range endpoint overflow");
            assert_eq!(
                bytes,
                &CONTENT[range_offset as usize..end as usize],
                "read contract: range mismatch"
            );
            self.context.record_check(
                "read/range",
                Some(FileSystemCapability::RangeRead),
                ContractCheckOutcome::Passed,
            );
            let outcome = match range_limit.maximum() {
                Some(maximum) if maximum < MAX_PROBE_BYTES => {
                    let over = maximum
                        .checked_add(1)
                        .expect("read contract: range limit successor overflow");
                    let error = self
                        .fixture
                        .file_system()
                        .open_reader(path, ReadOptions::default().with_length(Some(over)))
                        .expect_err("read/range-limit: declared range limit was ignored");
                    self.assert_error(
                        &error,
                        FsErrorKind::ResourceLimitExceeded,
                        FsOperation::OpenReader,
                        path,
                        None,
                    );
                    ContractCheckOutcome::Passed
                }
                Some(_) => ContractCheckOutcome::SkippedOptional {
                    reason: "range boundary exceeds the bounded probe budget".to_owned(),
                },
                None => ContractCheckOutcome::SkippedOptional {
                    reason: "range limit is unknown, inapplicable, or unbounded".to_owned(),
                },
            };
            self.context.record_check(
                "read/range-limit",
                Some(FileSystemCapability::RangeRead),
                outcome,
            );
        } else {
            let error = self
                .fixture
                .file_system()
                .open_reader(path, range)
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

        let if_match_support = if self.capable(FileSystemCapability::ConditionalRead) {
            self.fixture
                .case_support(FixtureCase::ReadIfMatch)
                .expect("conditional-read contract: If-Match case query failed")
        } else {
            FixtureSupport::Unsupported
        };
        let if_none_match_support = if self.capable(FileSystemCapability::ConditionalRead) {
            self.fixture
                .case_support(FixtureCase::ReadIfNoneMatch)
                .expect("conditional-read contract: If-None-Match case query failed")
        } else {
            FixtureSupport::Unsupported
        };
        if !self.capable(FileSystemCapability::ConditionalRead) {
            let current = ReadOptions::default()
                .with_if_match(Some(ResourceVersion::new("missing-capability-current")));
            let error = self
                .fixture
                .file_system()
                .open_reader(path, current)
                .expect_err("read contract: unadvertised current If-Match succeeded");
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
            let stale = ReadOptions::default()
                .with_if_match(Some(ResourceVersion::new("missing-capability-stale")));
            let error = self
                .fixture
                .file_system()
                .open_reader(path, stale)
                .expect_err("read contract: unadvertised stale If-Match succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::OpenReader,
                FileSystemCapability::ConditionalRead,
                "conditional-read contract",
            );
            self.context.record_check(
                "read/if-match-stale",
                Some(FileSystemCapability::ConditionalRead),
                ContractCheckOutcome::RejectedAsExpected,
            );
        } else if matches!(if_match_support, FixtureSupport::Unsupported) {
            for id in ["read/if-match-current", "read/if-match-stale"] {
                self.context.record_check(
                    id,
                    Some(FileSystemCapability::ConditionalRead),
                    ContractCheckOutcome::Unverified {
                        reason: "fixture cannot prepare If-Match case".to_owned(),
                    },
                );
            }
        } else {
            let current = self
                .fixture
                .resource_version(path)
                .expect("read contract: version observation failed");
            let stale = self
                .fixture
                .stale_resource_version(path)
                .expect("read contract: stale version observation failed");
            if let FixtureSupport::Supported(current) = &current {
                let bytes = self
                    .fixture
                    .file_system()
                    .read_all(
                        path,
                        ReadOptions::default().with_if_match(Some(current.clone())),
                        64,
                    )
                    .expect("read contract: current if-match failed");
                assert_eq!(bytes, CONTENT);
                self.context.record_check(
                    "read/if-match-current",
                    Some(FileSystemCapability::ConditionalRead),
                    ContractCheckOutcome::Passed,
                );
            }
            if matches!(&current, FixtureSupport::Unsupported) {
                self.context.record_check(
                    "read/if-match-current",
                    Some(FileSystemCapability::ConditionalRead),
                    ContractCheckOutcome::Unverified {
                        reason: "fixture current version unavailable".to_owned(),
                    },
                );
            }
            match (current, stale) {
                (FixtureSupport::Supported(_), FixtureSupport::Supported(stale)) => {
                    let error = self
                        .fixture
                        .file_system()
                        .read_all(
                            path,
                            ReadOptions::default().with_if_match(Some(stale)),
                            64,
                        )
                        .expect_err("read/if-match-stale: stale If-Match succeeded");
                    self.assert_error(
                        &error,
                        FsErrorKind::PreconditionFailed,
                        FsOperation::OpenReader,
                        path,
                        None,
                    );
                    self.context.record_check(
                        "read/if-match-stale",
                        Some(FileSystemCapability::ConditionalRead),
                        ContractCheckOutcome::Passed,
                    );
                }
                (FixtureSupport::Supported(_), FixtureSupport::Unsupported) => {
                    self.context.record_check(
                        "read/if-match-stale",
                        Some(FileSystemCapability::ConditionalRead),
                        ContractCheckOutcome::Unverified {
                            reason: "fixture stale version unavailable".to_owned(),
                        },
                    );
                }
                (FixtureSupport::Unsupported, _) => {
                    self.context.record_check(
                        "read/if-match-stale",
                        Some(FileSystemCapability::ConditionalRead),
                        ContractCheckOutcome::Unverified {
                            reason: "fixture current version unavailable".to_owned(),
                        },
                    );
                }
            }
        }

        if !self.capable(FileSystemCapability::ConditionalRead) {
            for (id, options, message) in [
                (
                    "read/if-none-match-current",
                    ReadOptions::default()
                        .with_if_none_match(Some(ResourceVersion::new("missing-capability-current"))),
                    "read contract: unadvertised current If-None-Match succeeded",
                ),
                (
                    "read/if-none-match-stale",
                    ReadOptions::default()
                        .with_if_none_match(Some(ResourceVersion::new("missing-capability-stale"))),
                    "read contract: unadvertised stale If-None-Match succeeded",
                ),
            ] {
                let error = self
                    .fixture
                    .file_system()
                    .open_reader(path, options)
                    .expect_err(message);
                self.assert_requirement_error(
                    &error,
                    FsOperation::OpenReader,
                    FileSystemCapability::ConditionalRead,
                    "conditional-read contract",
                );
                self.context.record_check(
                    id,
                    Some(FileSystemCapability::ConditionalRead),
                    ContractCheckOutcome::RejectedAsExpected,
                );
            }
        } else if matches!(if_none_match_support, FixtureSupport::Unsupported) {
            for id in ["read/if-none-match-current", "read/if-none-match-stale"] {
                self.context.record_check(
                    id,
                    Some(FileSystemCapability::ConditionalRead),
                    ContractCheckOutcome::Unverified {
                        reason: "fixture cannot prepare If-None-Match case".to_owned(),
                    },
                );
            }
        } else {
            let current = self
                .fixture
                .resource_version(path)
                .expect("read contract: version observation failed");
            let stale = self
                .fixture
                .stale_resource_version(path)
                .expect("read contract: stale version observation failed");
            match (current, stale) {
                (FixtureSupport::Supported(current), FixtureSupport::Supported(stale)) => {
                    let error = self
                        .fixture
                        .file_system()
                        .read_all(
                            path,
                            ReadOptions::default().with_if_none_match(Some(current)),
                            64,
                        )
                        .expect_err("read/if-none-match-current: current If-None-Match succeeded");
                    self.assert_error(
                        &error,
                        FsErrorKind::PreconditionFailed,
                        FsOperation::OpenReader,
                        path,
                        None,
                    );
                    self.context.record_check(
                        "read/if-none-match-current",
                        Some(FileSystemCapability::ConditionalRead),
                        ContractCheckOutcome::Passed,
                    );
                    let bytes = self
                        .fixture
                        .file_system()
                        .read_all(
                            path,
                            ReadOptions::default().with_if_none_match(Some(stale)),
                            64,
                        )
                        .expect("read/if-none-match-stale: stale If-None-Match failed");
                    assert_eq!(bytes, CONTENT);
                    self.context.record_check(
                        "read/if-none-match-stale",
                        Some(FileSystemCapability::ConditionalRead),
                        ContractCheckOutcome::Passed,
                    );
                }
                _ => {
                    for id in ["read/if-none-match-current", "read/if-none-match-stale"] {
                        self.context.record_check(
                            id,
                            Some(FileSystemCapability::ConditionalRead),
                            ContractCheckOutcome::Unverified {
                                reason: "fixture version observation unavailable".to_owned(),
                            },
                        );
                    }
                }
            }
        }

        self.assert_checksum(path, CONTENT);
    }

    fn assert_checksum(&mut self, path: &Path, content: &[u8]) {

        let checksummed = ReadOptions::default().with_checksum(ChecksumPolicy::Required);
        if self.capable(FileSystemCapability::ChecksumValidation) {
            let bytes = self
                .fixture
                .file_system()
                .read_all(path, checksummed, 64)
                .expect("read contract: advertised checksum validation failed");
            assert_eq!(bytes, content, "read/checksum: checksum bytes mismatch");
            self.context.record_check(
                "read/checksum",
                Some(FileSystemCapability::ChecksumValidation),
                ContractCheckOutcome::Passed,
            );
            let relative = self.context.relative_name("checksum-failure");
            match self
                .fixture
                .checksum_failure_case(&relative)
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
                        .expect_err("checksum-read contract: corrupted bytes were accepted");
                    self.assert_error(
                        &error,
                        FsErrorKind::DataCorruption,
                        FsOperation::OpenReader,
                        &failure_path,
                        None,
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
