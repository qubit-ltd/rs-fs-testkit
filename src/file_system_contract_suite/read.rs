// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Implements reader contracts.

use super::*;

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
        let range = ReadOptions::default()
            .with_offset(Some(5))
            .with_length(Some(8));
        if self.capable(FileSystemCapability::RangeRead) {
            let bytes = self
                .fixture
                .file_system()
                .read_all(path, range, 64)
                .expect("read contract: advertised range read failed");
            assert_eq!(bytes, b"contract", "read contract: range mismatch");
            self.context.record_check(
                "read/range",
                Some(FileSystemCapability::RangeRead),
                ContractCheckOutcome::Passed,
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
        }

        let version_support = self
            .fixture
            .resource_version(path)
            .expect("read contract: version observation failed");
        let stale_support = self
            .fixture
            .stale_resource_version(path)
            .expect("read contract: stale version observation failed");
        if !self.capable(FileSystemCapability::ConditionalRead) {
            let conditional = ReadOptions::default()
                .with_if_match(Some(ResourceVersion::new("contract-version")));
            let error = self
                .fixture
                .file_system()
                .open_reader(path, conditional)
                .expect_err("read contract: unadvertised conditional read succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::OpenReader,
                FileSystemCapability::ConditionalRead,
                "conditional-read contract",
            );
            for id in [
                "read/if-match-current",
                "read/if-match-stale",
                "read/if-none-match-current",
                "read/if-none-match-stale",
            ] {
                self.context.record_check(
                    id,
                    Some(FileSystemCapability::ConditionalRead),
                    ContractCheckOutcome::RejectedAsExpected,
                );
            }
        } else if let (FixtureSupport::Supported(current), FixtureSupport::Supported(stale)) =
            (version_support, stale_support)
        {
            let bytes = self
                .fixture
                .file_system()
                .read_all(
                    path,
                    ReadOptions::default().with_if_match(Some(current.clone())),
                    64,
                )
                .expect("read contract: current if-match failed");
            assert_eq!(bytes, b"read contract bytes");
            self.context.record_check(
                "read/if-match-current",
                Some(FileSystemCapability::ConditionalRead),
                ContractCheckOutcome::Passed,
            );
            let error = self
                .fixture
                .file_system()
                .read_all(
                    path,
                    ReadOptions::default().with_if_match(Some(stale.clone())),
                    64,
                )
                .expect_err("read/if-match-stale: stale if-match succeeded");
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
            let error = self
                .fixture
                .file_system()
                .read_all(
                    path,
                    ReadOptions::default().with_if_none_match(Some(current)),
                    64,
                )
                .expect_err("read contract: current if-none-match succeeded");
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
                .expect("read contract: stale if-none-match failed");
            assert_eq!(bytes, b"read contract bytes");
            self.context.record_check(
                "read/if-none-match-stale",
                Some(FileSystemCapability::ConditionalRead),
                ContractCheckOutcome::Passed,
            );
        } else {
            for id in [
                "read/if-match-current",
                "read/if-match-stale",
                "read/if-none-match-current",
                "read/if-none-match-stale",
            ] {
                self.context.record_check(
                    id,
                    Some(FileSystemCapability::ConditionalRead),
                    ContractCheckOutcome::Unverified {
                        reason: "fixture version observation unavailable".to_owned(),
                    },
                );
            }
        }

        let checksummed = ReadOptions::default().with_checksum(ChecksumPolicy::Required);
        if self.capable(FileSystemCapability::ChecksumValidation) {
            let bytes = self
                .fixture
                .file_system()
                .read_all(path, checksummed, 64)
                .expect("read contract: advertised checksum validation failed");
            assert_eq!(
                bytes, b"read contract bytes",
                "read/checksum: checksum bytes mismatch"
            );
            self.context.record_check(
                "read/checksum",
                Some(FileSystemCapability::ChecksumValidation),
                ContractCheckOutcome::Passed,
            );
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
        }
    }
}
