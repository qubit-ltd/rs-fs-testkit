// qubit-style: allow all
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements reader contracts.

use super::*;

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
                assert_eq!(actual, b"async bytes", "read contract: seeded bytes mismatch");
                let error = self
                    .fixture
                    .file_system()
                    .read_all(&path, Default::default(), 4)
                    .await
                    .expect_err("read contract: caller byte limit was ignored");
                self.assert_error(&error, FsErrorKind::ResourceLimitExceeded, FsOperation::Read, &path);
                path
            }
            FixtureSupport::Unsupported => {
                panic!("read contract: advertised capability requires fixture.seed_file support")
            }
        };
        self.assert_read_options(&path).await;
    }

    /// Checks asynchronous range, conditional, and checksum read guarantees.
    pub async fn assert_read_options(&self, path: &Path) {
        let range = ReadOptions::default().with_offset(Some(6)).with_length(Some(5));
        if self.capable(FileSystemCapability::RangeRead) {
            let bytes = self
                .fixture
                .file_system()
                .read_all(path, range, 64)
                .await
                .expect("read contract: advertised range read failed");
            assert_eq!(bytes, b"bytes", "read contract: range mismatch");
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
            assert!(
                matches!(version_support, FixtureSupport::Supported(_)),
                "conditional-read contract: advertised capability requires fixture.resource_version support"
            );
            let bytes = self
                .fixture
                .file_system()
                .read_all(path, conditional, 64)
                .await
                .expect("read contract: advertised conditional read failed");
            assert_eq!(bytes, b"async bytes");
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
        }
    }
}
