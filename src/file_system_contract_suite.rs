// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stateful synchronous filesystem provider contract suite.

use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::panic::resume_unwind;

use qubit_fs::AchievedAtomicity;
use qubit_fs::AtomicityRequirement;
use qubit_fs::ChecksumPolicy;
use qubit_fs::CopyConflictPolicy;
use qubit_fs::CopyMethod;
use qubit_fs::CopyMode;
use qubit_fs::CopyOptions;
use qubit_fs::CreateDirectoryOptions;
use qubit_fs::DeleteOptions;
use qubit_fs::DurabilityRequirement;
use qubit_fs::FileKind;
use qubit_fs::FileSystemCapability;
use qubit_fs::FsError;
use qubit_fs::FsErrorKind;
use qubit_fs::FsOperation;
use qubit_fs::ListOptions;
use qubit_fs::Path;
use qubit_fs::PathSemantics;
use qubit_fs::PersistFailureState;
use qubit_fs::PersistOptions;
use qubit_fs::ReadOptions;
use qubit_fs::RenameFailureState;
use qubit_fs::RenameOptions;
use qubit_fs::ResourceVersion;
use qubit_fs::ServerSidePreference;
use qubit_fs::TempDirectory;
use qubit_fs::TempDirectoryOptions;
use qubit_fs::TempFile;
use qubit_fs::TempFileOptions;
use qubit_fs::WriteDisposition;
use qubit_fs::WriteOptions;
use qubit_fs::WritePrecondition;
use qubit_io::Output;

use crate::FileSystemContract;
use crate::FileSystemFixture;
use crate::FixtureSupport;
use crate::contract_context::ContractContext;
use crate::internal::assert_error_with_source_or_target;
use crate::internal::assert_error_with_target;
use crate::internal::assert_unsupported_error;

/// Runs synchronous provider contracts against one isolated fixture.
///
/// # Type Parameters
///
/// * `'a` - Lifetime of the borrowed provider fixture.
#[must_use = "the suite must run at least one contract assertion"]
pub struct FileSystemContractSuite<'a> {
    /// Provider-owned fixture supplying the facade and observation hooks.
    fixture: &'a dyn FileSystemFixture,
    /// Property snapshot and cleanup state for the current suite run.
    context: ContractContext,
}

impl<'a> FileSystemContractSuite<'a> {
    /// Creates a stateful suite borrowing one isolated provider fixture.
    ///
    /// # Parameters
    ///
    /// * `fixture` - Isolated provider fixture exercised by the suite.
    ///
    /// # Returns
    ///
    /// A suite with a fresh context and captured property snapshot.
    #[inline]
    pub fn new(fixture: &'a dyn FileSystemFixture) -> Self {
        Self {
            fixture,
            context: ContractContext::new(fixture.file_system().properties()),
        }
    }

    /// Runs all synchronous contracts in their dependency-safe fixed order.
    ///
    /// # Panics
    ///
    /// Panics when the provider violates any contract or fixture setup and
    /// observation fails. Cleanup still runs before the panic is resumed.
    pub fn assert_all(mut self) {
        let result = catch_unwind(AssertUnwindSafe(|| {
            for contract in FileSystemContract::ALL {
                self.assert_contract_inner(contract);
            }
        }));
        self.finish();
        if let Err(payload) = result {
            resume_unwind(payload);
        }
    }

    /// Runs one named synchronous contract and always performs cleanup.
    ///
    /// # Panics
    ///
    /// Panics when the provider violates the selected contract or cleanup
    /// fails. Cleanup runs before an assertion panic is resumed.
    pub fn assert_contract(mut self, contract: FileSystemContract) {
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.assert_contract_inner(contract);
        }));
        self.finish();
        if let Err(payload) = result {
            resume_unwind(payload);
        }
    }

    /// Dispatches one named phase without changing cleanup ownership.
    fn assert_contract_inner(&mut self, contract: FileSystemContract) {
        match contract {
            FileSystemContract::Properties => self.assert_properties(),
            FileSystemContract::Stat => self.assert_stat(),
            FileSystemContract::Read => self.assert_read(),
            FileSystemContract::Write => self.assert_write(),
            FileSystemContract::List => self.assert_list(),
            FileSystemContract::CreateDirectory => {
                self.assert_create_directory()
            }
            FileSystemContract::Representations => {
                self.assert_representations()
            }
            FileSystemContract::Delete => self.assert_delete(),
            FileSystemContract::Copy => self.assert_copy(),
            FileSystemContract::Rename => self.assert_rename(),
            FileSystemContract::Append => self.assert_append(),
            FileSystemContract::RecursiveDelete => {
                self.assert_recursive_delete()
            }
            FileSystemContract::AtomicRename => self.assert_atomic_rename(),
            FileSystemContract::DurableRename => self.assert_durable_rename(),
            FileSystemContract::AtomicReplace => self.assert_atomic_replace(),
            FileSystemContract::DurableFileCopy => self.assert_durable_copy(),
            FileSystemContract::TempResources => self.assert_temp_resources(),
            FileSystemContract::ErrorContext => self.assert_error_context(),
        }
    }

    /// Cleans resources created by individually executed contract phases.
    ///
    /// # Panics
    ///
    /// Panics when a recorded resource cannot be inspected or deleted.
    pub fn finish(&mut self) {
        self.context.cleanup(self.fixture.file_system());
    }

    /// Checks immutable facade properties and fixture path compatibility.
    ///
    /// # Panics
    ///
    /// Panics when identifiers or capabilities are inconsistent, the fixture
    /// path is invalid, or the facade snapshot changes during the suite run.
    pub fn assert_properties(&mut self) {
        self.context.begin("properties");
        let properties = self.context.properties();
        let info = properties.info();
        assert!(
            !info.id().as_str().is_empty(),
            "properties contract: filesystem id is empty"
        );
        assert!(
            !info.provider_id().is_empty(),
            "properties contract: provider id is empty"
        );
        assert!(
            properties.capabilities().missing_dependency().is_none(),
            "properties contract: capability dependencies are inconsistent"
        );
        let path = self
            .fixture
            .path("contract-properties")
            .expect("properties contract: fixture path failed");
        properties
            .path_constraints()
            .validate(&path)
            .expect("properties contract: fixture path violates constraints");
        assert_eq!(
            properties.info(),
            self.fixture.file_system().properties().info(),
            "properties contract: snapshot changed"
        );
        assert_eq!(
            properties.capabilities(),
            self.fixture.file_system().properties().capabilities(),
            "properties contract: snapshot changed"
        );
    }

    /// Checks metadata behavior.
    ///
    /// # Panics
    ///
    /// Panics when missing-path errors are not structured correctly or seeded
    /// file metadata does not match the fixture content.
    pub fn assert_stat(&mut self) {
        self.context.begin("stat");
        let file_system = self.fixture.file_system();
        let missing = self.path("stat-missing");
        let error = file_system
            .stat(&missing)
            .expect_err("stat contract: missing path succeeded");
        self.assert_error(
            &error,
            FsErrorKind::NotFound,
            FsOperation::Stat,
            &missing,
            None,
        );

        if let FixtureSupport::Supported(path) =
            self.seed("stat-file", b"stateful stat")
        {
            self.context.record_created(path.clone());
            let metadata = file_system
                .stat(&path)
                .expect("stat contract: seeded file is not statable");
            assert!(
                metadata.is_file_like(),
                "stat contract: seeded resource is not file-like"
            );
            assert_eq!(
                metadata.len(),
                Some(13),
                "stat contract: file length mismatch"
            );
        }
    }

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
                .expect_err(
                    "read contract: unadvertised reader open succeeded",
                );
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
        let path =
            self.required_seed("read-file", b"read contract bytes", "read");
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
    fn assert_read_options(&self, path: &Path) {
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
        let conditional = ReadOptions::default().with_if_match(Some(
            match &version_support {
                FixtureSupport::Supported(version) => version.clone(),
                FixtureSupport::Unsupported => {
                    ResourceVersion::new("contract-version")
                }
            },
        ));
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
                .expect_err(
                    "read contract: unadvertised conditional read succeeded",
                );
            self.assert_requirement_error(
                &error,
                FsOperation::OpenReader,
                FileSystemCapability::ConditionalRead,
                "conditional-read contract",
            );
        }

        let checksummed =
            ReadOptions::default().with_checksum(ChecksumPolicy::Required);
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
                .expect_err(
                    "read contract: unadvertised checksum validation succeeded",
                );
            self.assert_requirement_error(
                &error,
                FsOperation::OpenReader,
                FileSystemCapability::ChecksumValidation,
                "checksum-read contract",
            );
        }
    }

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
                .expect_err(
                    "writer contract: unadvertised writer open succeeded",
                );
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
            assert_eq!(
                bytes_written, 7,
                "writer contract: byte count mismatch"
            );
        }
        match self
            .fixture
            .read_file(&path)
            .expect("writer contract: fixture observation failed")
        {
            FixtureSupport::Supported(bytes) => {
                assert_eq!(
                    bytes, b"written",
                    "I/O contract: write was not published"
                )
            }
            FixtureSupport::Unsupported => {
                panic!(
                    "writer contract: Write capability requires fixture.read_file support"
                )
            }
        }
        self.assert_write_options(&path);
    }

    /// Checks write dispositions, abort behavior, and conditional writes.
    fn assert_write_options(&mut self, existing: &Path) {
        let create_new = WriteOptions::default()
            .with_disposition(WriteDisposition::CreateNew);
        let failure = self
            .fixture
            .file_system()
            .write_all(existing, b"unexpected", create_new)
            .expect_err(
                "writer contract: create-new replaced an existing target",
            );
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
        let conditional = WriteOptions::default()
            .with_precondition(WritePrecondition::IfAbsent);
        if self.capable(FileSystemCapability::ConditionalWrite) {
            self.context.record_created(conditional_path.clone());
            self.fixture
                .file_system()
                .write_all(
                    &conditional_path,
                    b"conditional",
                    conditional.clone(),
                )
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
                .expect_err(
                    "writer contract: unadvertised conditional write succeeded",
                );
            self.assert_requirement_error(
                &error,
                FsOperation::OpenWriter,
                FileSystemCapability::ConditionalWrite,
                "conditional-write contract",
            );
        }
    }

    /// Checks directory listing behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, child enumeration, prefix filtering,
    /// pagination, or requested metadata violates the listing contract.
    pub fn assert_list(&mut self) {
        self.context.begin("list");
        let root = self.path("list-root");
        if !self.capable(FileSystemCapability::List) {
            let error = self
                .fixture
                .file_system()
                .list(&root, Default::default())
                .expect_err("list contract: unadvertised listing succeeded");
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::List,
                &root,
                None,
            );
            assert_eq!(
                error.required_capability(),
                Some(FileSystemCapability::List),
                "list contract: missing required-capability context"
            );
            return;
        }
        let first = self.required_seed("list-root/first", b"first", "list");
        self.context.record_created(first.clone());
        let second = self.required_seed("list-root/second", b"second", "list");
        self.context.record_created(second.clone());
        let mut stream = self
            .fixture
            .file_system()
            .list(&root, Default::default())
            .expect("list contract: cannot open namespace");
        let mut actual = Vec::new();
        while let Some(entry) =
            stream.next_entry().expect("list contract: stream error")
        {
            actual.push(entry.path);
        }
        actual.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        let mut expected = vec![first, second];
        expected.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        assert_eq!(actual, expected, "list contract: direct children mismatch");

        let nested =
            self.required_seed("list-root/prefixed/nested", b"nested", "list");
        self.context.record_created(nested.clone());
        let nested_second = self.required_seed(
            "list-root/prefixed/second",
            b"second nested",
            "list",
        );
        self.context.record_created(nested_second.clone());
        let prefix = self
            .fixture
            .list_prefix(&root, "prefixed")
            .expect("list contract: fixture prefix failed");
        let mut stream = self
            .fixture
            .file_system()
            .list(
                &root,
                ListOptions::default()
                    .with_include_metadata(true)
                    .with_prefix(Some(prefix))
                    .with_page_size(Some(1)),
            )
            .expect("list contract: prefix listing failed");
        let mut prefixed = Vec::new();
        while let Some(entry) = stream
            .next_entry()
            .expect("list contract: prefix stream error")
        {
            assert!(
                entry.metadata.is_some(),
                "list contract: requested entry metadata is missing"
            );
            prefixed.push(entry.path);
        }
        prefixed.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        let mut expected = vec![nested, nested_second];
        let prefix_entry = Path::parse(
            expected[0]
                .as_str()
                .rsplit_once('/')
                .expect("list contract: nested path must have a parent")
                .0,
        )
        .expect("list contract: generated prefix path must be valid");
        if prefixed.contains(&prefix_entry) {
            expected.push(prefix_entry);
        }
        expected.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        assert_eq!(
            prefixed, expected,
            "list contract: paged prefix results mismatch"
        );
    }

    /// Checks directory creation behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, directory publication, metadata, or
    /// existing-directory handling violates the creation contract.
    pub fn assert_create_directory(&mut self) {
        self.context.begin("create_directory");
        if !self.capable(FileSystemCapability::CreateDirectory) {
            let path = self.path("create-directory-unavailable");
            let error = self
                .fixture
                .file_system()
                .create_directory(&path, CreateDirectoryOptions::default())
                .expect_err("create-directory contract: unadvertised creation succeeded");
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::CreateDir,
                &path,
                None,
            );
            assert_eq!(
                error.required_capability(),
                Some(FileSystemCapability::CreateDirectory),
                "create-directory contract: missing required-capability context"
            );
            return;
        }
        let path = self.path("created-directory");
        self.context.record_created(path.clone());
        self.fixture
            .file_system()
            .create_directory(&path, CreateDirectoryOptions::default())
            .expect("namespace contract: directory creation failed");
        let metadata = self
            .fixture
            .file_system()
            .stat(&path)
            .expect("namespace contract: created directory is missing");
        assert!(
            metadata.is_directory_like(),
            "namespace contract: created path is not a directory"
        );
        let outcome = self
            .fixture
            .file_system()
            .create_directory(
                &path,
                CreateDirectoryOptions::default().with_exists_ok(true),
            )
            .expect("namespace contract: existing directory was not accepted");
        assert!(
            outcome.already_existed(),
            "namespace contract: existing directory outcome was not reported"
        );
        let parent = self.path("created-recursive-parent");
        let child = self.path("created-recursive-parent/child");
        self.context.record_created(parent);
        self.context.record_created(child.clone());
        let outcome = self
            .fixture
            .file_system()
            .create_directory(
                &child,
                CreateDirectoryOptions::default().with_recursive(true),
            )
            .expect("namespace contract: recursive directory creation failed");
        if let Some(created_ancestors) = outcome.created_ancestors() {
            assert!(
                created_ancestors > 0,
                "namespace contract: recursive ancestor count is zero"
            );
        }
    }

    /// Checks advertised empty-directory and symbolic-link representations.
    pub fn assert_representations(&mut self) {
        self.context.begin("representations");
        if self.capable(FileSystemCapability::EmptyDirectory) {
            let relative = self.context.relative_name("empty-directory");
            let path = match self
                .fixture
                .seed_empty_directory(&relative)
                .expect("representation contract: empty-directory setup failed")
            {
                FixtureSupport::Supported(path) => path,
                FixtureSupport::Unsupported => panic!(
                    "representation contract: EmptyDirectory requires fixture.seed_empty_directory support"
                ),
            };
            self.context.record_created(path.clone());
            let metadata = self.fixture.file_system().stat(&path).expect(
                "representation contract: empty directory is not statable",
            );
            assert!(
                metadata.is_directory_like(),
                "representation contract: empty directory is not directory-like"
            );
        }
        if self.capable(FileSystemCapability::Symlink) {
            let relative = self.context.relative_name("symlink");
            let path = match self
                .fixture
                .seed_symlink(&relative)
                .expect("representation contract: symlink setup failed")
            {
                FixtureSupport::Supported(path) => path,
                FixtureSupport::Unsupported => {
                    panic!(
                        "representation contract: Symlink requires fixture.seed_symlink support"
                    )
                }
            };
            self.context.record_created(path.clone());
            let metadata = self
                .fixture
                .file_system()
                .stat(&path)
                .expect("representation contract: symlink is not statable");
            assert_eq!(
                metadata.kind(),
                &FileKind::Symlink,
                "representation contract: seeded link kind mismatch"
            );
        }
    }

    /// Checks deletion behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, deletion, existence observation, or
    /// structured error context violates the deletion contract.
    pub fn assert_delete(&mut self) {
        self.context.begin("delete");
        if !self.capable(FileSystemCapability::Delete) {
            let path = self.path("delete-unavailable");
            let error = self
                .fixture
                .file_system()
                .delete_file(&path, DeleteOptions::default())
                .expect_err("delete contract: unadvertised deletion succeeded");
            self.assert_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::Delete,
                &path,
                None,
            );
            assert_eq!(
                error.required_capability(),
                Some(FileSystemCapability::Delete),
                "delete contract: missing required-capability context"
            );
            return;
        }
        let path = self.required_seed("delete-file", b"delete", "delete");
        self.context.record_created(path.clone());
        let outcome = self
            .fixture
            .file_system()
            .delete_file(&path, DeleteOptions::default())
            .expect("delete contract: deletion failed");
        assert!(
            !outcome.already_missing(),
            "delete contract: existing file was reported missing"
        );
        if let Some(deleted_entries) = outcome.deleted_entries() {
            assert!(
                deleted_entries > 0,
                "delete contract: deleted count is zero"
            );
        }
        assert!(
            !self
                .fixture
                .file_system()
                .exists(&path)
                .expect("delete contract: exists failed"),
            "delete contract: deleted file remained"
        );
        self.assert_delete_options();
    }

    /// Checks missing-ok and conditional deletion semantics.
    fn assert_delete_options(&mut self) {
        let missing = self.path("delete-missing-ok");
        let outcome = self
            .fixture
            .file_system()
            .delete_file(
                &missing,
                DeleteOptions::default().with_missing_ok(true),
            )
            .expect("delete contract: missing-ok deletion failed");
        assert!(
            outcome.already_missing(),
            "delete contract: missing-ok outcome did not report absence"
        );

        let path = self.path("delete-conditional");
        let options = DeleteOptions::default()
            .with_if_match(Some(ResourceVersion::new("contract-version")));
        if self.capable(FileSystemCapability::ConditionalDelete) {
            let path = self.required_seed(
                "delete-conditional",
                b"conditional delete",
                "conditional-delete",
            );
            self.context.record_created(path.clone());
            let version = match self
                .fixture
                .resource_version(&path)
                .expect("delete contract: version observation failed")
            {
                FixtureSupport::Supported(version) => version,
                FixtureSupport::Unsupported => panic!(
                    "conditional-delete contract: advertised capability requires fixture.resource_version support"
                ),
            };
            self.fixture
                .file_system()
                .delete_file(
                    &path,
                    DeleteOptions::default().with_if_match(Some(version)),
                )
                .expect(
                    "delete contract: advertised conditional delete failed",
                );
            assert!(!self.fixture.file_system().exists(&path).expect(
                "delete contract: conditional target observation failed"
            ));
        } else {
            let error = self
                .fixture
                .file_system()
                .delete_file(&path, options)
                .expect_err("delete contract: unadvertised conditional delete succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::Delete,
                FileSystemCapability::ConditionalDelete,
                "conditional-delete contract",
            );
        }
    }

    /// Checks native and fallback copy behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, copy publication, method reporting,
    /// directory recursion, or a required native fixture case is invalid.
    pub fn assert_copy(&mut self) {
        self.context.begin("copy");
        if !self.capable(FileSystemCapability::Copy) {
            let source = self.path("copy-unavailable");
            let target = self.path("copy-unavailable-target");
            let error = self
                .fixture
                .file_system()
                .copy(&source, &target, CopyOptions::default())
                .expect_err("copy contract: unavailable fallback succeeded");
            self.assert_error(
                error.error(),
                FsErrorKind::UnsupportedCapability,
                FsOperation::Copy,
                &source,
                Some(&target),
            );
            assert_eq!(
                error.error().required_capability(),
                Some(FileSystemCapability::Read),
                "copy contract: fallback missing read-capability context"
            );
            return;
        }
        let source = self.required_seed("copy-source", b"copy bytes", "copy");
        self.context.record_created(source.clone());
        let target = self.path("copy-target");
        self.context.record_created(target.clone());
        let outcome = self
            .fixture
            .file_system()
            .copy(&source, &target, CopyOptions::default())
            .expect("copy contract: fallback copy failed");
        match outcome.method() {
            CopyMethod::Streamed => assert!(
                outcome.used_fallback(),
                "copy contract: streamed copy was not reported as fallback"
            ),
            CopyMethod::Native
            | CopyMethod::Clone
            | CopyMethod::ServerSide
            | CopyMethod::Mixed => {
                assert!(
                    !outcome.used_fallback(),
                    "copy contract: completed fast path was reported as fallback"
                )
            }
        }
        assert_eq!(
            outcome.stats().bytes,
            10,
            "copy contract: copied byte count mismatch"
        );
        assert_eq!(
            outcome.stats().files + outcome.stats().objects,
            1,
            "copy contract: copied resource count mismatch"
        );
        self.assert_bytes(
            &source,
            b"copy bytes",
            "copy contract: source was modified",
        );
        self.assert_bytes(
            &target,
            b"copy bytes",
            "copy contract: target bytes mismatch",
        );
        self.assert_copy_conflicts(&source);
        if self.capable(FileSystemCapability::CreateDirectory) {
            let directory_source = self.path("copy-directory-source");
            self.fixture
                .file_system()
                .create_directory(
                    &directory_source,
                    CreateDirectoryOptions::default(),
                )
                .expect("copy contract: directory source creation failed");
            self.context.record_created(directory_source.clone());
            let directory_child = self.required_seed(
                "copy-directory-source/child",
                b"directory copy",
                "copy",
            );
            let directory_target = self.path("copy-directory-target");
            self.context.record_created(directory_target.clone());
            let target_child = self.path("copy-directory-target/child");
            self.context.record_created(target_child.clone());
            self.fixture
                .file_system()
                .copy(&directory_source, &directory_target, CopyOptions::tree())
                .expect("copy contract: directory copy failed");
            self.assert_bytes(
                &target_child,
                b"directory copy",
                "copy contract: directory child bytes mismatch",
            );
            self.context.record_created(directory_child);
        }
        if self.capable(FileSystemCapability::ServerSideCopy) {
            match self
                .fixture
                .copy_fast_path_case(CopyMethod::ServerSide)
                .expect("copy contract: fixture fast-path setup failed")
            {
                FixtureSupport::Supported(case) => {
                    self.context.record_created(case.source().clone());
                    self.context.record_created(case.target().clone());
                    let outcome = self
                        .fixture
                        .file_system()
                        .copy(
                            case.source(),
                            case.target(),
                            case.options().clone(),
                        )
                        .expect("copy contract: native case failed");
                    assert_eq!(
                        outcome.method(),
                        CopyMethod::ServerSide,
                        "copy contract: reported method mismatch"
                    );
                    assert!(
                        !outcome.used_fallback(),
                        "copy contract: native case unexpectedly fell back"
                    );
                }
                FixtureSupport::Unsupported => panic!(
                    "copy contract: advertised native capability lacks an applicable fixture case"
                ),
            }
        } else {
            let source = self.path("copy-server-side-unavailable-source");
            let target = self.path("copy-server-side-unavailable-target");
            let failure = self
                .fixture
                .file_system()
                .copy(
                    &source,
                    &target,
                    CopyOptions::default()
                        .with_server_side(ServerSidePreference::Require),
                )
                .expect_err(
                    "copy contract: unadvertised server-side copy succeeded",
                );
            self.assert_requirement_error(
                failure.error(),
                FsOperation::Copy,
                FileSystemCapability::ServerSideCopy,
                "server-side-copy contract",
            );
        }
    }

    /// Checks destination conflict policies and copy statistics.
    fn assert_copy_conflicts(&mut self, source: &Path) {
        let target = self.required_seed(
            "copy-conflict-target",
            b"existing",
            "copy-conflict",
        );
        self.context.record_created(target.clone());
        let failure = self
            .fixture
            .file_system()
            .copy(source, &target, CopyOptions::file())
            .expect_err("copy contract: default conflict replaced target");
        self.assert_error(
            failure.error(),
            FsErrorKind::AlreadyExists,
            FsOperation::Copy,
            source,
            Some(&target),
        );
        self.assert_bytes(
            &target,
            b"existing",
            "copy contract: failed conflict changed target",
        );

        let skipped = self
            .fixture
            .file_system()
            .copy(
                source,
                &target,
                CopyOptions::default()
                    .with_mode(CopyMode::File)
                    .with_conflict(CopyConflictPolicy::Skip),
            )
            .expect("copy contract: skip conflict failed");
        assert_eq!(
            skipped.stats().skipped,
            1,
            "copy contract: skipped count mismatch"
        );
        self.assert_bytes(
            &target,
            b"existing",
            "copy contract: skipped copy changed target",
        );

        let overwritten = self
            .fixture
            .file_system()
            .copy(
                source,
                &target,
                CopyOptions::default()
                    .with_mode(CopyMode::File)
                    .with_conflict(CopyConflictPolicy::Overwrite),
            )
            .expect("copy contract: overwrite conflict failed");
        assert_eq!(
            overwritten.stats().overwritten,
            1,
            "copy contract: overwritten count mismatch"
        );
        self.assert_bytes(
            &target,
            b"copy bytes",
            "copy contract: overwrite bytes mismatch",
        );
    }

    /// Checks rename behavior.
    ///
    /// # Panics
    ///
    /// Panics when capability preflight, publication, source removal, target
    /// content, or structured error context violates the rename contract.
    pub fn assert_rename(&mut self) {
        self.context.begin("rename");
        if !self.capable(FileSystemCapability::Rename) {
            let source = self.path("rename-unavailable-source");
            let target = self.path("rename-unavailable-target");
            let failure = self
                .fixture
                .file_system()
                .rename(&source, &target, RenameOptions::default())
                .expect_err("rename contract: unadvertised rename succeeded");
            self.assert_error(
                failure.error(),
                FsErrorKind::UnsupportedCapability,
                FsOperation::Rename,
                &source,
                Some(&target),
            );
            assert_eq!(
                failure.error().required_capability(),
                Some(FileSystemCapability::Rename),
                "rename contract: missing required-capability context"
            );
            return;
        }
        let source = self.required_seed("rename-source", b"rename", "rename");
        self.context.record_created(source.clone());
        let target = self.path("rename-target");
        self.context.record_created(target.clone());
        let outcome = self
            .fixture
            .file_system()
            .rename(&source, &target, RenameOptions::default())
            .expect("rename contract: rename failed");
        assert_eq!(
            outcome.source(),
            &source,
            "rename contract: source context mismatch"
        );
        assert_eq!(
            outcome.target(),
            &target,
            "rename contract: target context mismatch"
        );
        assert!(
            !self
                .fixture
                .file_system()
                .exists(&source)
                .expect("rename contract: source exists failed"),
            "rename contract: source remained after success"
        );
        assert!(
            self.fixture
                .file_system()
                .exists(&target)
                .expect("rename contract: target exists failed"),
            "rename contract: target missing after success"
        );
        self.assert_rename_conflicts();
    }

    /// Checks rename destination conflicts and explicit overwrite.
    fn assert_rename_conflicts(&mut self) {
        let source = self.required_seed(
            "rename-conflict-source",
            b"rename source",
            "rename-conflict",
        );
        let target = self.required_seed(
            "rename-conflict-target",
            b"rename target",
            "rename-conflict",
        );
        self.context.record_created(source.clone());
        self.context.record_created(target.clone());
        let failure = self
            .fixture
            .file_system()
            .rename(&source, &target, RenameOptions::default())
            .expect_err("rename contract: default conflict replaced target");
        assert_eq!(
            failure.state(),
            RenameFailureState::Unchanged,
            "rename contract: conflict failure state mismatch"
        );
        self.assert_error(
            failure.error(),
            FsErrorKind::AlreadyExists,
            FsOperation::Rename,
            &source,
            Some(&target),
        );
        self.assert_bytes(
            &source,
            b"rename source",
            "rename contract: conflict removed source",
        );
        self.assert_bytes(
            &target,
            b"rename target",
            "rename contract: conflict changed target",
        );

        self.fixture
            .file_system()
            .rename(
                &source,
                &target,
                RenameOptions::default().with_overwrite(true),
            )
            .expect("rename contract: overwrite failed");
        assert!(
            !self
                .fixture
                .file_system()
                .exists(&source)
                .expect("rename contract: overwrite source observation failed")
        );
        self.assert_bytes(
            &target,
            b"rename source",
            "rename contract: overwrite target mismatch",
        );
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
        let options =
            WriteOptions::default().with_disposition(WriteDisposition::Append);
        if !self.capable(FileSystemCapability::Append) {
            let error = self
                .fixture
                .file_system()
                .open_writer(&path, options)
                .expect_err(
                    "append contract: unadvertised append preflight succeeded",
                );
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

    /// Checks recursive directory removal when the provider advertises it.
    ///
    /// # Panics
    ///
    /// Panics when recursive-delete preflight or descendant removal violates
    /// the advertised capability, or fixture setup fails.
    pub fn assert_recursive_delete(&mut self) {
        self.context.begin("recursive_delete");
        let root = self.path("recursive-delete-root");
        let options = DeleteOptions::default().with_recursive(true);
        if !self.capable(FileSystemCapability::RecursiveDelete) {
            let error = self
                .fixture
                .file_system()
                .delete_directory(&root, options)
                .expect_err("recursive-delete contract: unadvertised preflight succeeded");
            self.assert_requirement_error(
                &error,
                FsOperation::Delete,
                FileSystemCapability::RecursiveDelete,
                "recursive-delete contract",
            );
            return;
        }
        if self.capable(FileSystemCapability::CreateDirectory) {
            self.fixture
                .file_system()
                .create_directory(&root, CreateDirectoryOptions::default())
                .expect("recursive-delete contract: root creation failed");
        }
        let child = self.required_seed(
            "recursive-delete-root/child",
            b"child",
            "recursive-delete",
        );
        self.context.record_created(child.clone());
        self.context.record_created(root.clone());
        self.fixture
            .file_system()
            .delete_directory(&root, options)
            .expect("recursive-delete contract: recursive removal failed");
        assert!(
            !self.fixture.file_system().exists(&root).expect(
                "recursive-delete contract: root existence check failed"
            ),
            "recursive-delete contract: root remained after removal"
        );
        assert!(
            !self.fixture.file_system().exists(&child).expect(
                "recursive-delete contract: child existence check failed"
            ),
            "recursive-delete contract: child remained after removal"
        );
    }

    /// Checks required-atomic rename publication when advertised.
    ///
    /// # Panics
    ///
    /// Panics when atomic-rename preflight, publication, or reported atomicity
    /// violates the advertised capability.
    pub fn assert_atomic_rename(&mut self) {
        self.context.begin("atomic_rename");
        let source = self.path("atomic-rename-source");
        let target = self.path("atomic-rename-target");
        let options = RenameOptions::default()
            .with_atomicity(AtomicityRequirement::Required);
        if !self.capable(FileSystemCapability::AtomicRename) {
            let failure = self
                .fixture
                .file_system()
                .rename(&source, &target, options)
                .expect_err(
                    "atomic-rename contract: unadvertised preflight succeeded",
                );
            self.assert_requirement_error(
                failure.error(),
                FsOperation::Rename,
                FileSystemCapability::AtomicRename,
                "atomic-rename contract",
            );
            return;
        }
        let source = self.required_seed(
            "atomic-rename-source",
            b"atomic rename",
            "atomic-rename",
        );
        let target = self.path("atomic-rename-target");
        self.context.record_created(source.clone());
        self.context.record_created(target.clone());
        let outcome = self
            .fixture
            .file_system()
            .rename(&source, &target, options)
            .expect("atomic-rename contract: required-atomic rename failed");
        assert_eq!(
            outcome.atomicity(),
            AchievedAtomicity::Atomic,
            "atomic-rename contract: required operation reported non-atomic publication"
        );
    }

    /// Checks required-durable rename publication when advertised.
    pub fn assert_durable_rename(&mut self) {
        self.context.begin("durable_rename");
        let source = self.path("durable-rename-source");
        let target = self.path("durable-rename-target");
        let options = RenameOptions::default()
            .with_durability(DurabilityRequirement::Required);
        if !self.capable(FileSystemCapability::DurableRename) {
            let failure = self
                .fixture
                .file_system()
                .rename(&source, &target, options)
                .expect_err(
                    "durable-rename contract: unadvertised preflight succeeded",
                );
            self.assert_requirement_error(
                failure.error(),
                FsOperation::Rename,
                FileSystemCapability::DurableRename,
                "durable-rename contract",
            );
            return;
        }
        let source = self.required_seed(
            "durable-rename-source",
            b"durable rename",
            "durable-rename",
        );
        self.context.record_created(source.clone());
        self.context.record_created(target.clone());
        let outcome = self
            .fixture
            .file_system()
            .rename(&source, &target, options)
            .expect("durable-rename contract: required rename failed");
        assert!(
            outcome.durable(),
            "durable-rename contract: required operation reported non-durable publication"
        );
        self.assert_bytes(
            &target,
            b"durable rename",
            "durable-rename contract: target bytes mismatch",
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
        let options = WriteOptions::default()
            .with_atomicity(AtomicityRequirement::Required);
        if !self.capable(FileSystemCapability::AtomicReplace) {
            let error = self
                .fixture
                .file_system()
                .open_writer(&path, options)
                .expect_err(
                    "atomic-replace contract: unadvertised preflight succeeded",
                );
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

    /// Checks required-durable copy publication when the provider advertises
    /// it.
    ///
    /// # Panics
    ///
    /// Panics when durable-copy preflight, publication, reported durability,
    /// or target content violates the advertised capability.
    pub fn assert_durable_copy(&mut self) {
        self.context.begin("durable_copy");
        let source = self.path("durable-copy-source");
        let target = self.path("durable-copy-target");
        let options = CopyOptions::default()
            .with_durability(DurabilityRequirement::Required);
        if !self.capable(FileSystemCapability::DurableFileCopy) {
            let failure = self
                .fixture
                .file_system()
                .copy(&source, &target, options)
                .expect_err(
                    "durable-copy contract: unadvertised preflight succeeded",
                );
            self.assert_requirement_error(
                failure.error(),
                FsOperation::Copy,
                FileSystemCapability::DurableFileCopy,
                "durable-copy contract",
            );
            return;
        }
        let source = self.required_seed(
            "durable-copy-source",
            b"durable copy",
            "durable-copy",
        );
        let target = self.path("durable-copy-target");
        self.context.record_created(source.clone());
        self.context.record_created(target.clone());
        let outcome = self
            .fixture
            .file_system()
            .copy(&source, &target, options)
            .expect("durable-copy contract: required-durable copy failed");
        assert!(
            outcome.durable(),
            "durable-copy contract: required operation reported non-durable publication"
        );
        self.assert_bytes(
            &target,
            b"durable copy",
            "durable-copy contract: target bytes mismatch",
        );
    }

    /// Checks temporary resource lifecycle behavior.
    ///
    /// # Panics
    ///
    /// Panics when temporary resource options, cleanup, persistence,
    /// replacement, or reported atomicity violates an advertised capability.
    pub fn assert_temp_resources(&mut self) {
        self.context.begin("temp_resources");
        let file_system = self.fixture.file_system();
        if self.capable(FileSystemCapability::TempFile) {
            let incompatible_parent =
                match self.context.properties().info().path_semantics() {
                    PathSemantics::Hierarchical => {
                        Path::parse_literal("/temp-invalid-parent")
                    }
                    _ => Path::parse("/temp-invalid-parent"),
                }
                .expect("incompatible temporary parent should parse");
            let error = match file_system.create_temp_file(
                TempFileOptions::default()
                    .with_parent(Some(incompatible_parent.clone())),
            ) {
                Ok(_) => panic!("temp-file contract: invalid parent succeeded"),
                Err(error) => error,
            };
            assert_eq!(
                error.kind(),
                FsErrorKind::InvalidPath,
                "temp-file contract: parent validation kind mismatch"
            );
            assert_eq!(
                error.operation(),
                FsOperation::CreateTemp,
                "temp-file contract: parent validation operation mismatch"
            );
            assert_eq!(
                error.path(),
                Some(&incompatible_parent),
                "temp-file contract: parent validation path mismatch"
            );
            let parent = self.path("temp-file-parent");
            if self.capable(FileSystemCapability::CreateDirectory) {
                file_system
                    .create_directory(
                        &parent,
                        CreateDirectoryOptions::default(),
                    )
                    .expect("temp-file contract: parent creation failed");
                self.context.record_created(parent.clone());
            }
            let options = TempFileOptions::default()
                .with_parent(
                    self.capable(FileSystemCapability::CreateDirectory)
                        .then_some(parent),
                )
                .with_prefix("contract-file-".to_owned())
                .with_suffix(".tmp".to_owned());
            let mut temporary = file_system
                .create_temp_file(options)
                .expect("temp-file contract: create failed");
            let source = temporary.path().clone();
            assert!(
                source.as_str().contains("/contract-file-"),
                "temp-file contract: requested prefix was ignored"
            );
            assert!(
                source.as_str().ends_with(".tmp"),
                "temp-file contract: requested suffix was ignored"
            );
            temporary
                .cleanup()
                .expect("temp-file contract: cleanup failed");
            assert!(
                !file_system
                    .exists(&source)
                    .expect("temp-file contract: source exists failed"),
                "temp-file contract: cleanup retained source"
            );
            let mut temporary = file_system
                .create_temp_file(Default::default())
                .expect("temp-file contract: persist setup failed");
            let target = self.path("temp-persisted-file");
            self.context.record_created(target.clone());
            self.assert_temp_persist(&mut temporary, &target, "temp-file");
        } else {
            let error = file_system
                .create_temp_file(TempFileOptions::default())
                .expect_err(
                    "temp-file contract: unadvertised creation succeeded",
                );
            self.assert_pathless_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::CreateTemp,
            );
        }
        if self.capable(FileSystemCapability::TempDirectory) {
            let parent = self.path("temp-directory-parent");
            if self.capable(FileSystemCapability::CreateDirectory) {
                file_system
                    .create_directory(
                        &parent,
                        CreateDirectoryOptions::default(),
                    )
                    .expect("temp-directory contract: parent creation failed");
                self.context.record_created(parent.clone());
            }
            let options = TempDirectoryOptions::default()
                .with_parent(
                    self.capable(FileSystemCapability::CreateDirectory)
                        .then_some(parent),
                )
                .with_prefix("contract-directory-".to_owned())
                .with_suffix(".tmp".to_owned());
            let mut temporary = file_system
                .create_temp_directory(options)
                .expect("temp-directory contract: create failed");
            let source = temporary.path().clone();
            assert!(
                source.as_str().contains("/contract-directory-"),
                "temp-directory contract: requested prefix was ignored"
            );
            assert!(
                source.as_str().ends_with(".tmp"),
                "temp-directory contract: requested suffix was ignored"
            );
            temporary
                .cleanup()
                .expect("temp-directory contract: cleanup failed");
            assert!(
                !file_system
                    .exists(&source)
                    .expect("temp-directory contract: source exists failed"),
                "temp-directory contract: cleanup retained source"
            );
            let mut temporary = file_system
                .create_temp_directory(Default::default())
                .expect("temp-directory contract: persist setup failed");
            let target = self.path("temp-persisted-directory");
            self.context.record_created(target.clone());
            self.assert_temp_directory_persist(&mut temporary, &target);
            if self.capable(FileSystemCapability::CreateDirectory) {
                self.assert_temp_directory_overwrite();
            }
        } else {
            let error = file_system
                .create_temp_directory(TempDirectoryOptions::default())
                .expect_err(
                    "temp-directory contract: unadvertised creation succeeded",
                );
            self.assert_pathless_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::CreateTemp,
            );
        }
    }

    /// Checks structured filesystem error context and redaction behavior.
    ///
    /// # Panics
    ///
    /// Panics when a missing-path error omits or misreports its structured
    /// kind, operation, or path context.
    pub fn assert_error_context(&mut self) {
        self.context.begin("error_context");
        let path = self.path("error-context-missing");
        let error = self
            .fixture
            .file_system()
            .stat(&path)
            .expect_err("error contract: missing path succeeded");
        self.assert_error(
            &error,
            FsErrorKind::NotFound,
            FsOperation::Stat,
            &path,
            None,
        );
    }

    /// Resolves a fixture path or identifies the contract that could not set
    /// up.
    ///
    /// # Parameters
    ///
    /// * `relative` - Resource name relative to the current contract phase.
    ///
    /// # Returns
    ///
    /// The provider path mapped by the fixture.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot map the generated relative name.
    #[inline]
    fn path(&self, relative: &str) -> Path {
        let relative = self.context.relative_name(relative);
        self.fixture
            .path(&relative)
            .expect("contract: fixture path failed")
    }

    /// Returns whether the immutable snapshot declares a capability.
    ///
    /// # Parameters
    ///
    /// * `capability` - Capability to query in the captured snapshot.
    ///
    /// # Returns
    ///
    /// `true` when the provider advertises the capability.
    #[inline(always)]
    fn capable(&self, capability: FileSystemCapability) -> bool {
        self.context
            .properties()
            .capabilities()
            .supports(capability)
    }

    /// Seeds a resource and makes support mandatory for the requested
    /// capability.
    ///
    /// # Parameters
    ///
    /// * `relative` - Resource name relative to the current contract phase.
    /// * `bytes` - Exact content to publish.
    /// * `contract` - Contract label used in failure diagnostics.
    ///
    /// # Returns
    ///
    /// The provider path of the seeded file.
    ///
    /// # Panics
    ///
    /// Panics when fixture setup fails or the required seed hook is
    /// unsupported.
    #[inline]
    fn required_seed(
        &self,
        relative: &str,
        bytes: &[u8],
        contract: &str,
    ) -> Path {
        match self.seed(relative, bytes) {
            FixtureSupport::Supported(path) => path,
            FixtureSupport::Unsupported => panic!(
                "{contract} contract: advertised capability requires fixture.seed_file support"
            ),
        }
    }

    /// Delegates out-of-band resource preparation to the fixture.
    ///
    /// # Parameters
    ///
    /// * `relative` - Resource name relative to the current contract phase.
    /// * `bytes` - Exact content to publish.
    ///
    /// # Returns
    ///
    /// The fixture's support result and seeded path, when available.
    ///
    /// # Panics
    ///
    /// Panics when provider-specific fixture setup returns an error.
    #[inline]
    fn seed(&self, relative: &str, bytes: &[u8]) -> FixtureSupport<Path> {
        let relative = self.context.relative_name(relative);
        self.fixture
            .seed_file(&relative, bytes)
            .expect("contract: fixture seed failed")
    }

    /// Reads a provider-owned probe through the fixture and checks exact bytes.
    ///
    /// # Parameters
    ///
    /// * `path` - Provider path to observe.
    /// * `expected` - Exact expected content.
    /// * `message` - Assertion message used when content differs.
    ///
    /// # Panics
    ///
    /// Panics when observation fails, is unsupported, or returns different
    /// content.
    fn assert_bytes(&self, path: &Path, expected: &[u8], message: &str) {
        match self
            .fixture
            .read_file(path)
            .expect("copy contract: fixture observation failed")
        {
            FixtureSupport::Supported(actual) => {
                assert_eq!(actual, expected, "{message}")
            }
            FixtureSupport::Unsupported => {
                panic!(
                    "copy contract: Copy capability requires fixture.read_file support"
                )
            }
        }
    }

    /// Verifies file persistence publication and the atomic-required preflight.
    ///
    /// # Parameters
    ///
    /// * `temporary` - Temporary file whose publication is tested.
    /// * `target` - Requested persistent destination.
    /// * `label` - Contract label used in diagnostics.
    ///
    /// # Panics
    ///
    /// Panics when persistence, atomicity preflight, ownership retention, or
    /// cleanup violates the temporary-file contract.
    fn assert_temp_persist(
        &self,
        temporary: &mut TempFile,
        target: &Path,
        label: &str,
    ) {
        self.assert_temp_file_persist_result(temporary, target, label);
        if !self.capable(FileSystemCapability::AtomicTempPersist) {
            let mut retry = self
                .fixture
                .file_system()
                .create_temp_file(Default::default())
                .expect("temp-file contract: atomic preflight setup failed");
            let source = retry.path().clone();
            let failure = retry
                .persist(
                    &self.path("temp-required-atomic-file"),
                    PersistOptions::default(),
                )
                .expect_err("temp-file contract: unadvertised required atomic persist succeeded");
            assert_eq!(
                failure.state(),
                PersistFailureState::NotPublished,
                "temp-file contract: failed preflight changed publication responsibility"
            );
            assert_eq!(
                failure.error().kind(),
                FsErrorKind::RequirementNotMet,
                "temp-file contract: failed preflight kind mismatch"
            );
            assert_eq!(
                failure.error().operation(),
                FsOperation::PersistTemp,
                "temp-file contract: failed preflight operation mismatch"
            );
            assert_eq!(
                failure.error().path(),
                Some(&source),
                "temp-file contract: failed preflight source mismatch"
            );
            assert_eq!(
                failure.error().target(),
                Some(&self.path("temp-required-atomic-file")),
                "temp-file contract: failed preflight target mismatch"
            );
            assert_eq!(
                failure.error().provider(),
                Some(self.context.properties().info().provider_id()),
                "temp-file contract: failed preflight provider mismatch"
            );
            assert!(
                self.fixture
                    .file_system()
                    .exists(&source)
                    .expect("temp-file contract: source exists failed"),
                "temp-file contract: required atomic preflight removed source"
            );
            retry
                .cleanup()
                .expect("temp-file contract: retained source cleanup failed");
        }
    }

    /// Verifies directory persistence publication and the atomic-required
    /// preflight.
    ///
    /// # Parameters
    ///
    /// * `temporary` - Temporary directory whose publication is tested.
    /// * `target` - Requested persistent destination.
    ///
    /// # Panics
    ///
    /// Panics when persistence, atomicity preflight, ownership retention, or
    /// cleanup violates the temporary-directory contract.
    fn assert_temp_directory_persist(
        &self,
        temporary: &mut TempDirectory,
        target: &Path,
    ) {
        let outcome = temporary
            .persist(
                target,
                PersistOptions::default().with_atomicity(
                    if self.capable(FileSystemCapability::AtomicTempPersist) {
                        AtomicityRequirement::Required
                    } else {
                        AtomicityRequirement::Preferred
                    },
                ),
            )
            .expect("temp-directory contract: persist failed");
        assert_eq!(
            outcome.target(),
            target,
            "temp-directory contract: persist target mismatch"
        );
        if self.capable(FileSystemCapability::AtomicTempPersist) {
            assert_eq!(
                outcome.atomicity(),
                AchievedAtomicity::Atomic,
                "temp-directory contract: required operation reported non-atomic publication"
            );
        }
        assert!(
            self.fixture
                .file_system()
                .exists(target)
                .expect("temp-directory contract: target exists failed"),
            "temp-directory contract: persist did not publish target"
        );
        if !self.capable(FileSystemCapability::AtomicTempPersist) {
            let mut retry = self
                .fixture
                .file_system()
                .create_temp_directory(Default::default())
                .expect(
                    "temp-directory contract: atomic preflight setup failed",
                );
            let source = retry.path().clone();
            let failure = retry
                .persist(
                    &self.path("temp-required-atomic-directory"),
                    PersistOptions::default(),
                )
                .expect_err(
                    "temp-directory contract: unadvertised required atomic persist succeeded",
                );
            assert_eq!(
                failure.state(),
                PersistFailureState::NotPublished,
                "temp-directory contract: failed preflight changed publication responsibility"
            );
            assert_eq!(
                failure.error().kind(),
                FsErrorKind::RequirementNotMet,
                "temp-directory contract: failed preflight kind mismatch"
            );
            assert_eq!(
                failure.error().operation(),
                FsOperation::PersistTemp,
                "temp-directory contract: failed preflight operation mismatch"
            );
            assert_eq!(
                failure.error().path(),
                Some(&source),
                "temp-directory contract: failed preflight source mismatch"
            );
            assert_eq!(
                failure.error().target(),
                Some(&self.path("temp-required-atomic-directory")),
                "temp-directory contract: failed preflight target mismatch"
            );
            assert_eq!(
                failure.error().provider(),
                Some(self.context.properties().info().provider_id()),
                "temp-directory contract: failed preflight provider mismatch"
            );
            assert!(
                self.fixture
                    .file_system()
                    .exists(&source)
                    .expect("temp-directory contract: source exists failed"),
                "temp-directory contract: required atomic preflight removed source"
            );
            retry.cleanup().expect(
                "temp-directory contract: retained source cleanup failed",
            );
        }
    }

    /// Verifies a temporary directory replaces an existing empty directory
    /// when the caller explicitly allows replacement.
    ///
    /// # Panics
    ///
    /// Panics when setup, overwrite publication, outcome reporting, or target
    /// observation violates the temporary-directory contract.
    fn assert_temp_directory_overwrite(&mut self) {
        let file_system = self.fixture.file_system();
        let target = self.path("temp-overwritten-directory");
        self.context.record_created(target.clone());
        file_system
            .create_directory(&target, CreateDirectoryOptions::default())
            .expect(
                "temp-directory overwrite contract: destination setup failed",
            );
        let mut temporary = file_system
            .create_temp_directory(Default::default())
            .expect(
                "temp-directory overwrite contract: temporary creation failed",
            );
        let outcome = temporary
            .persist(
                &target,
                PersistOptions::default()
                    .with_overwrite(true)
                    .with_atomicity(
                        if self.capable(FileSystemCapability::AtomicTempPersist)
                        {
                            AtomicityRequirement::Required
                        } else {
                            AtomicityRequirement::Preferred
                        },
                    ),
            )
            .expect("temp-directory overwrite contract: persist failed");
        assert_eq!(
            outcome.target(),
            &target,
            "temp-directory overwrite contract: persist target mismatch"
        );
        assert!(
            file_system.exists(&target).expect(
                "temp-directory overwrite contract: target exists failed"
            ),
            "temp-directory overwrite contract: replacement did not publish target"
        );
    }

    /// Persists one temporary file using the strongest guarantee it advertises.
    ///
    /// # Parameters
    ///
    /// * `temporary` - Temporary file whose publication is tested.
    /// * `target` - Requested persistent destination.
    /// * `label` - Contract label used in diagnostics.
    ///
    /// # Panics
    ///
    /// Panics when persistence fails, reports the wrong destination or
    /// atomicity, or does not publish the target.
    fn assert_temp_file_persist_result(
        &self,
        temporary: &mut TempFile,
        target: &Path,
        label: &str,
    ) {
        let outcome = temporary
            .persist(
                target,
                PersistOptions::default().with_atomicity(
                    if self.capable(FileSystemCapability::AtomicTempPersist) {
                        AtomicityRequirement::Required
                    } else {
                        AtomicityRequirement::Preferred
                    },
                ),
            )
            .expect("temp-file contract: persist failed");
        assert_eq!(
            outcome.target(),
            target,
            "{label} contract: persist target mismatch"
        );
        if self.capable(FileSystemCapability::AtomicTempPersist) {
            assert_eq!(
                outcome.atomicity(),
                AchievedAtomicity::Atomic,
                "{label} contract: required operation reported non-atomic publication"
            );
        }
        assert!(
            self.fixture
                .file_system()
                .exists(target)
                .expect("temp-file contract: target exists failed"),
            "{label} contract: persist did not publish target"
        );
    }

    /// Validates public error context without exposing provider implementation
    /// details.
    ///
    /// # Parameters
    ///
    /// * `error` - Actual filesystem error.
    /// * `kind` - Expected error classification.
    /// * `operation` - Expected public operation.
    /// * `path` - Expected source path.
    /// * `target` - Expected destination path, when applicable.
    ///
    /// # Panics
    ///
    /// Panics when any expected structured field differs.
    fn assert_error(
        &self,
        error: &FsError,
        kind: FsErrorKind,
        operation: FsOperation,
        path: &Path,
        target: Option<&Path>,
    ) {
        let provider = Some(self.context.properties().info().provider_id());
        if kind == FsErrorKind::UnsupportedCapability && target.is_none() {
            assert_unsupported_error(
                error,
                kind,
                operation,
                Some(path),
                provider,
                error.required_capability(),
            );
        } else if kind == FsErrorKind::AlreadyExists
            && let Some(target) = target
        {
            assert_error_with_source_or_target(
                error,
                kind,
                operation,
                path,
                target,
                provider,
                error.required_capability(),
            );
        } else {
            assert_error_with_target(
                error,
                kind,
                operation,
                Some(path),
                target,
                provider,
                error.required_capability(),
            );
        }
    }

    /// Validates an operation error that has no logical input path.
    fn assert_pathless_error(
        &self,
        error: &FsError,
        kind: FsErrorKind,
        operation: FsOperation,
    ) {
        assert_unsupported_error(
            error,
            kind,
            operation,
            None,
            Some(self.context.properties().info().provider_id()),
            error.required_capability(),
        );
    }

    /// Validates option-derived capability preflight errors.
    ///
    /// # Parameters
    ///
    /// * `error` - Actual filesystem error.
    /// * `operation` - Expected public operation.
    /// * `capability` - Capability required by the rejected options.
    /// * `contract` - Contract label used in diagnostics.
    ///
    /// # Panics
    ///
    /// Panics when the error kind, operation, or required capability differs.
    fn assert_requirement_error(
        &self,
        error: &FsError,
        operation: FsOperation,
        capability: FileSystemCapability,
        contract: &str,
    ) {
        let _ = contract;
        assert_unsupported_error(
            error,
            FsErrorKind::RequirementNotMet,
            operation,
            error.path(),
            Some(self.context.properties().info().provider_id()),
            Some(capability),
        );
    }
}
