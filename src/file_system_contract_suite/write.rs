// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! // Implements writer and publication contracts.

use super::*;

impl<'a> FileSystemContractSuite<'a> {
    /// Checks writer behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, publication, fixture observation, or
    /// structured error context violates the writer contract.
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
            assert_eq!(
                error.required_capability(),
                Some(FileSystemCapability::Write),
                "writer contract: missing required-capability context"
            );
            return;
        }
        let path = self.path("write-file");
        self.context.record_created(path.clone());
        let outcome = self
            .fixture
            .file_system()
            .write_all(&path, b"written", WriteOptions::default())
            .expect("writer contract: write failed");
        if let Some(bytes_written) = outcome.bytes_written() {
            assert_eq!(bytes_written, 7, "writer contract: byte count mismatch");
        }
        match self
            .fixture
            .read_file(&path)
            .expect("writer contract: fixture observation failed")
        {
            FixtureSupport::Supported(bytes) => {
                assert_eq!(bytes, b"written", "I/O contract: write was not published")
            }
            FixtureSupport::Unsupported => {
                panic!("writer contract: Write capability requires fixture.read_file support")
            }
        }
        self.assert_write_options(&path);
    }

    /// Checks write dispositions, abort behavior, and conditional writes.
    pub(super) fn assert_write_options(&mut self, existing: &Path) {
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
        assert!(
            matches!(
                failure.error().operation(),
                FsOperation::OpenWriter | FsOperation::CommitWriter
            ),
            "writer contract: create-new failed at an unrelated operation"
        );
        self.assert_bytes(
            existing,
            b"written",
            "writer contract: failed create-new changed the target",
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
                .expect("writer contract: aborted path observation failed"),
            "writer contract: abort published the target"
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
            assert!(
                matches!(
                    failure.error().operation(),
                    FsOperation::OpenWriter | FsOperation::CommitWriter
                ),
                "writer contract: conditional write failed at an unrelated operation"
            );
            self.assert_bytes(
                &conditional_path,
                b"conditional",
                "writer contract: failed condition changed target bytes",
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
        }
    }

    /// Checks append writes when the provider advertises that guarantee.
    ///
    /// # Panics
    ///
    /// Panics when append preflight or publication violates the advertised
    /// capability, or fixture setup and observation fail.
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
            return;
        }
        let path = self.required_seed("append-target", b"before", "append");
        self.context.record_created(path.clone());
        self.fixture
            .file_system()
            .write_all(&path, b"-after", options)
            .expect("append contract: append failed");
        self.assert_bytes(
            &path,
            b"before-after",
            "append contract: existing bytes were not retained",
        );
    }

    /// Checks required-atomic replacement when the provider advertises it.
    ///
    /// # Panics
    ///
    /// Panics when atomic-replacement preflight, publication, or reported
    /// atomicity violates the advertised capability.
    pub fn assert_atomic_replace(&mut self) {
        self.context.begin("atomic_replace");
        let path = self.path("atomic-replace-target");
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
            return;
        }
        self.context.record_created(path.clone());
        let outcome = self
            .fixture
            .file_system()
            .write_all(&path, b"atomic replacement", options)
            .expect("atomic-replace contract: required-atomic write failed");
        assert_eq!(
            outcome.atomicity(),
            AchievedAtomicity::Atomic,
            "atomic-replace contract: required operation reported non-atomic publication"
        );
    }
}
