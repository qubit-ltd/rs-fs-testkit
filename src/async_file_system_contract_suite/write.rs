// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements writer and publication contracts.

use super::*;

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
            return;
        }
        let path = self.path("async-write");
        self.context.record_created(path.clone());
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&path, Default::default())
            .await
            .expect("write contract: writer open failed");
        writer
            .write_fully_async(b"async written")
            .await
            .expect("write contract: writer rejected bytes");
        let outcome = writer
            .commit_async()
            .await
            .expect("write contract: writer commit failed");
        if let Some(bytes_written) = outcome.bytes_written() {
            assert_eq!(bytes_written, 13, "writer contract: byte count mismatch");
        }
        match self
            .fixture
            .read_file(&path)
            .await
            .expect("write contract: fixture observation failed")
        {
            FixtureSupport::Supported(bytes) => {
                assert_eq!(bytes, b"async written", "write contract: bytes were not published")
            }
            FixtureSupport::Unsupported => {
                panic!("write contract: Write capability requires fixture.read_file support")
            }
        }
        self.assert_write_options(&path).await;
    }

    /// Checks asynchronous write dispositions, abort, and conditions.
    pub async fn assert_write_options(&mut self, existing: &Path) {
        let create_new = WriteOptions::default().with_disposition(WriteDisposition::CreateNew);
        let error = match self.fixture.file_system().open_writer(existing, create_new).await {
            Ok(mut writer) => {
                writer
                    .write_fully_async(b"unexpected")
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
            b"async written",
            "writer contract: failed create-new changed the target",
        )
        .await;

        let mut writer = self
            .fixture
            .file_system()
            .open_writer(existing, WriteOptions::default())
            .await
            .expect("writer contract: replacement writer open failed");
        writer
            .write_fully_async(b"replaced")
            .await
            .expect("writer contract: replacement writer rejected bytes");
        writer
            .commit_async()
            .await
            .expect("writer contract: replacement commit failed");
        self.assert_bytes(existing, b"replaced", "writer contract: replacement bytes mismatch")
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
            .write_fully_async(b"aborted")
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
            self.context.record_created(conditional_path.clone());
            let mut writer = self
                .fixture
                .file_system()
                .open_writer(&conditional_path, conditional.clone())
                .await
                .expect("writer contract: conditional writer open failed");
            writer
                .write_fully_async(b"conditional")
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
                        .write_fully_async(b"unexpected")
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
                b"conditional",
                "writer contract: failed condition changed target bytes",
            )
            .await;
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
            return;
        }
        let path = self.required_seed("async-append-target", b"before", "append").await;
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&path, options)
            .await
            .expect("append contract: writer open failed");
        writer
            .write_fully_async(b"-after")
            .await
            .expect("append contract: write failed");
        writer.commit_async().await.expect("append contract: commit failed");
        self.assert_bytes(
            &path,
            b"before-after",
            "append contract: existing bytes were not retained",
        )
        .await;
    }

    /// Checks asynchronous atomic replacement publication when advertised.
    ///
    /// # Panics
    ///
    /// Panics when atomic-replacement preflight, publication, or reported
    /// atomicity violates the advertised capability.
    pub async fn assert_atomic_replace(&mut self) {
        self.context.begin("atomic_replace");
        let path = self.path("async-atomic-replace-target");
        let options = WriteOptions::default().with_atomicity(AtomicityRequirement::Required);
        if !self.capable(FileSystemCapability::AtomicReplace) {
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
            return;
        }
        self.context.record_created(path.clone());
        let mut writer = self
            .fixture
            .file_system()
            .open_writer(&path, options)
            .await
            .expect("atomic-replace contract: writer open failed");
        writer
            .write_fully_async(b"atomic replacement")
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
    }
}
