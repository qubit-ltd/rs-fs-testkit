// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Implements writer and publication contracts.

use super::*;

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
            return;
        }
        let path = self.path("write-file");
        let initial: &[u8] = if self
            .context
            .properties()
            .limits()
            .max_write_bytes()
            .maximum()
            .is_some_and(|limit| limit < 7)
        {
            b"x"
        } else {
            b"written"
        };
        self.context.record_created(path.clone());
        let outcome = self
            .fixture
            .file_system()
            .write_all(&path, initial, WriteOptions::default())
            .expect("writer contract: write failed");
        if let Some(bytes_written) = outcome.bytes_written() {
            assert_eq!(bytes_written, 7);
        }
        self.assert_bytes(&path, initial, "writer contract: write was not published");
        self.context.record_check(
            "write/basic",
            Some(FileSystemCapability::Write),
            ContractCheckOutcome::Passed,
        );
        self.assert_write_options(&path, initial);
    }

    /// Checks write dispositions, abort behavior, and conditional writes.
    pub fn assert_write_options(&mut self, existing: &Path, initial: &[u8]) {
        let create_new = WriteOptions::default().with_disposition(WriteDisposition::CreateNew);
        let failure = self
            .fixture
            .file_system()
            .write_all(existing, b"unexpected", create_new)
            .expect_err("writer contract: create-new replaced an existing target");
        self.assert_error(
            failure.error(),
            FsErrorKind::AlreadyExists,
            failure.error().operation(),
            existing,
            None,
        );
        self.assert_bytes(
            existing,
            initial,
            "writer contract: failed create-new changed target",
        );

        self.fixture
            .file_system()
            .write_all(existing, b"replaced", WriteOptions::default())
            .expect("writer contract: replacement failed");
        self.assert_bytes(
            existing,
            b"replaced",
            "writer contract: replacement bytes mismatch",
        );

        let aborted_path = self.path("write-aborted");
        self.context.record_created(aborted_path.clone());
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&aborted_path, WriteOptions::default())
            .expect("writer contract: abort writer open failed");
        Output::write_fully(&mut writer, b"aborted")
            .expect("writer contract: abort writer rejected bytes");
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
        if self.capable(FileSystemCapability::ConditionalWrite) {
            self.context.record_created(conditional_path.clone());
            self.fixture
                .file_system()
                .write_all(&conditional_path, b"conditional", conditional.clone())
                .expect("writer contract: advertised conditional write failed");
            let failure = self
                .fixture
                .file_system()
                .write_all(&conditional_path, b"unexpected", conditional)
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
                b"conditional",
                "writer contract: failed condition changed target bytes",
            );
            self.context.record_check(
                "write/if-absent",
                Some(FileSystemCapability::ConditionalWrite),
                ContractCheckOutcome::Passed,
            );
            let current = match self
                .fixture
                .resource_version(&conditional_path)
                .expect("write contract: version observation failed")
            {
                FixtureSupport::Supported(version) => version,
                FixtureSupport::Unsupported => ResourceVersion::new("v1"),
            };
            self.fixture
                .file_system()
                .write_all(
                    &conditional_path,
                    b"current",
                    WriteOptions::default()
                        .with_precondition(WritePrecondition::IfMatch(current.clone())),
                )
                .expect("writer contract: current if-match failed");
            let stale = match self
                .fixture
                .stale_resource_version(&conditional_path)
                .expect("write contract: stale version observation failed")
            {
                FixtureSupport::Supported(version) => version,
                FixtureSupport::Unsupported => ResourceVersion::new("v0"),
            };
            let failure = self
                .fixture
                .file_system()
                .write_all(
                    &conditional_path,
                    b"stale",
                    WriteOptions::default().with_precondition(WritePrecondition::IfMatch(stale)),
                )
                .expect_err("write/if-match: stale if-match succeeded");
            self.assert_error(
                failure.error(),
                FsErrorKind::PreconditionFailed,
                failure.error().operation(),
                &conditional_path,
                None,
            );
            self.assert_bytes(
                &conditional_path,
                b"current",
                "writer contract: stale if-match changed target",
            );
            self.context.record_check(
                "write/if-match",
                Some(FileSystemCapability::ConditionalWrite),
                ContractCheckOutcome::Passed,
            );
        } else {
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
            self.context.record_check(
                "write/if-match",
                Some(FileSystemCapability::ConditionalWrite),
                ContractCheckOutcome::RejectedAsExpected,
            );
        }
        if self.capable(FileSystemCapability::AtomicReplace) {
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
        if self.capable(FileSystemCapability::DurableWrite) {
            let durable_path = self.path("write-durable");
            self.context.record_created(durable_path.clone());
            let outcome = self
                .fixture
                .file_system()
                .write_all(
                    &durable_path,
                    b"durable",
                    WriteOptions::default().with_durability(DurabilityRequirement::Required),
                )
                .expect("write/durable: durable write failed");
            assert!(
                outcome.durable(),
                "write/durable: required durable write was not durable"
            );
            self.assert_bytes(
                &durable_path,
                b"durable",
                "write/durable: durable bytes mismatch",
            );
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
        self.fixture
            .file_system()
            .write_all(&path, b"-after", options)
            .expect("append contract: append failed");
        self.assert_bytes(
            &path,
            b"before-after",
            "append contract: existing bytes were not retained",
        );
        self.context.record_check(
            "append/basic",
            Some(FileSystemCapability::Append),
            ContractCheckOutcome::Passed,
        );
    }

    /// Checks required-atomic replacement against an existing target.
    pub fn assert_atomic_replace(&mut self) {
        self.context.begin("atomic_replace");
        let path = self.required_seed("atomic-replace-target", b"old", "atomic-replace");
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
        let outcome = self
            .fixture
            .file_system()
            .write_all(&path, b"new", options)
            .expect("atomic-replace contract: required-atomic write failed");
        assert_eq!(outcome.atomicity(), AchievedAtomicity::Atomic);
        self.assert_bytes(
            &path,
            b"new",
            "write/atomic-replace-existing: replacement bytes mismatch",
        );
        self.context.record_check(
            "write/atomic-replace-existing",
            Some(FileSystemCapability::AtomicReplace),
            ContractCheckOutcome::Passed,
        );
    }
}
