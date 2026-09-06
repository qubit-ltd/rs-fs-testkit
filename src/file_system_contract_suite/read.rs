// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! // Implements reader contracts.

use super::*;

impl<'a> FileSystemContractSuite<'a> {
    /// Checks reader behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, seeded reads, byte limits, or
    /// structured error context violates the reader contract.
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
                Some(FileSystemCapability::Read),
                "read contract: missing required-capability context"
            );
            return;
        }
        let path = self.required_seed("read-file", b"read contract bytes", "read");
        self.context.record_created(path.clone());
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
        self.assert_read_options(&path);
    }

    /// Checks range, conditional, and checksum read guarantees.
    pub(super) fn assert_read_options(&self, path: &Path) {
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
        }

        let version_support = self
            .fixture
            .resource_version(path)
            .expect("read contract: version observation failed");
        let conditional = ReadOptions::default().with_if_match(Some(match &version_support {
            FixtureSupport::Supported(version) => version.clone(),
            FixtureSupport::Unsupported => ResourceVersion::new("contract-version"),
        }));
        if self.capable(FileSystemCapability::ConditionalRead) {
            assert!(
                matches!(version_support, FixtureSupport::Supported(_)),
                "conditional-read contract: advertised capability requires fixture.resource_version support"
            );
            let bytes = self
                .fixture
                .file_system()
                .read_all(path, conditional, 64)
                .expect("read contract: advertised conditional read failed");
            assert_eq!(bytes, b"read contract bytes");
        } else {
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
        }

        let checksummed = ReadOptions::default().with_checksum(ChecksumPolicy::Required);
        if self.capable(FileSystemCapability::ChecksumValidation) {
            let bytes = self
                .fixture
                .file_system()
                .read_all(path, checksummed, 64)
                .expect("read contract: advertised checksum validation failed");
            assert_eq!(bytes, b"read contract bytes");
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
        }
    }
}
