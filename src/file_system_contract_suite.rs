// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow all -- contract behavior is covered by the conforming and
// fault matrices.
//! Stateful synchronous filesystem provider contract suite.

use qubit_fs::{
    AtomicityRequirement,
    CopyMethod,
    CopyOptions,
    CreateDirectoryOptions,
    DeleteOptions,
    FileKind,
    FileSystemCapability,
    FsErrorKind,
    FsOperation,
    PersistFailureState,
    PersistOptions,
    RenameOptions,
    TempDirectory,
    TempFile,
    WriteOptions,
};

use crate::contract_context::ContractContext;
use crate::{
    FileSystemFixture,
    FixtureSupport,
};

/// Runs synchronous provider contracts against one isolated fixture.
pub struct FileSystemContractSuite<'a> {
    fixture: &'a dyn FileSystemFixture,
    context: ContractContext,
}

impl<'a> FileSystemContractSuite<'a> {
    /// Creates a stateful suite borrowing one isolated provider fixture.
    #[must_use]
    pub fn new(fixture: &'a dyn FileSystemFixture) -> Self {
        Self {
            fixture,
            context: ContractContext::new(fixture.file_system().properties()),
        }
    }

    /// Runs all synchronous contracts in their dependency-safe fixed order.
    pub fn assert_all(mut self) {
        self.assert_properties();
        self.assert_stat();
        self.assert_read();
        self.assert_write();
        self.assert_list();
        self.assert_create_directory();
        self.assert_delete();
        self.assert_copy();
        self.assert_rename();
        self.assert_temp_resources();
        self.assert_error_context();
        self.context.cleanup(self.fixture.file_system());
    }

    /// Checks immutable facade properties and fixture path compatibility.
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
        assert_ne!(
            info.id().as_str(),
            info.provider_id(),
            "properties contract: filesystem id equals provider id"
        );
        assert!(
            properties.capabilities().missing_dependency().is_none(),
            "properties contract: capability dependencies are inconsistent"
        );
        let path =
            self.fixture
                .path("contract-properties")
                .unwrap_or_else(|error| {
                    panic!("properties contract: fixture path failed: {error}")
                });
        properties
            .path_constraints()
            .validate(&path)
            .unwrap_or_else(|error| {
                panic!("properties contract: fixture path violates constraints: {error}")
            });
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
            assert_eq!(
                metadata.kind,
                FileKind::File,
                "stat contract: file kind mismatch"
            );
            assert_eq!(
                metadata.len,
                Some(13),
                "stat contract: file length mismatch"
            );
        }
    }

    /// Checks reader behavior.
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
    }

    /// Checks writer behavior.
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
        self.fixture
            .file_system()
            .write_all(&path, b"written", WriteOptions::default())
            .unwrap_or_else(|failure| {
                panic!("writer contract: write failed: {}", failure.error())
            });
        match self.fixture.read_file(&path).unwrap_or_else(|error| {
            panic!("writer contract: fixture observation failed: {error}")
        }) {
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
        self.context.record_created(path);
    }

    /// Checks directory listing behavior.
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
            .unwrap_or_else(|error| {
                panic!("list contract: cannot open namespace: {error}")
            });
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
    }

    /// Checks directory creation behavior.
    pub fn assert_create_directory(&mut self) {
        self.context.begin("create_directory");
        if !self.capable(FileSystemCapability::CreateDirectory) {
            return;
        }
        let path = self.path("created-directory");
        self.fixture
            .file_system()
            .create_directory(&path, CreateDirectoryOptions::default())
            .unwrap_or_else(|error| {
                panic!("namespace contract: directory creation failed: {error}")
            });
        let metadata = self
            .fixture
            .file_system()
            .stat(&path)
            .expect("namespace contract: created directory is missing");
        assert!(
            metadata.is_directory_like(),
            "namespace contract: created path is not a directory"
        );
        self.context.record_created(path);
    }

    /// Checks deletion behavior.
    pub fn assert_delete(&mut self) {
        self.context.begin("delete");
        if !self.capable(FileSystemCapability::Delete) {
            return;
        }
        let path = self.required_seed("delete-file", b"delete", "delete");
        self.context.record_created(path.clone());
        self.fixture
            .file_system()
            .delete_file(&path, DeleteOptions::default())
            .unwrap_or_else(|error| {
                panic!("delete contract: deletion failed: {error}")
            });
        assert!(
            !self
                .fixture
                .file_system()
                .exists(&path)
                .expect("delete contract: exists failed"),
            "delete contract: deleted file remained"
        );
    }

    /// Checks native and fallback copy behavior.
    pub fn assert_copy(&mut self) {
        self.context.begin("copy");
        if !self.capable(FileSystemCapability::Copy) {
            return;
        }
        let source = self.required_seed("copy-source", b"copy bytes", "copy");
        self.context.record_created(source.clone());
        let target = self.path("copy-target");
        let outcome = self
            .fixture
            .file_system()
            .copy(&source, &target, CopyOptions::default())
            .unwrap_or_else(|failure| {
                panic!(
                    "copy contract: fallback copy failed: {}",
                    failure.error()
                )
            });
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
        self.context.record_created(target);
        if self.capable(FileSystemCapability::ServerSideCopy) {
            match self
                .fixture
                .copy_fast_path_case(CopyMethod::ServerSide)
                .unwrap_or_else(|error| {
                    panic!(
                        "copy contract: fixture fast-path setup failed: {error}"
                    )
                }) {
                FixtureSupport::Supported(case) => {
                    let outcome = self
                        .fixture
                        .file_system()
                        .copy(
                            case.source(),
                            case.target(),
                            case.options().clone(),
                        )
                        .unwrap_or_else(|failure| {
                            panic!(
                                "copy contract: native case failed: {}",
                                failure.error()
                            )
                        });
                    assert_eq!(
                        outcome.method(),
                        CopyMethod::ServerSide,
                        "copy contract: reported method mismatch"
                    );
                    assert!(
                        !outcome.used_fallback(),
                        "copy contract: native case unexpectedly fell back"
                    );
                    self.context.record_created(case.source().clone());
                    self.context.record_created(case.target().clone());
                }
                FixtureSupport::Unsupported => panic!(
                    "copy contract: advertised native capability lacks an applicable fixture case"
                ),
            }
        }
    }

    /// Checks rename behavior.
    pub fn assert_rename(&mut self) {
        self.context.begin("rename");
        if !self.capable(FileSystemCapability::Rename) {
            return;
        }
        let source = self.required_seed("rename-source", b"rename", "rename");
        self.context.record_created(source.clone());
        let target = self.path("rename-target");
        let outcome = self
            .fixture
            .file_system()
            .rename(&source, &target, RenameOptions::default())
            .unwrap_or_else(|failure| {
                panic!("rename contract: rename failed: {}", failure.error())
            });
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
        self.context.record_created(target);
    }

    /// Checks temporary resource lifecycle behavior.
    pub fn assert_temp_resources(&mut self) {
        self.context.begin("temp_resources");
        let file_system = self.fixture.file_system();
        if self.capable(FileSystemCapability::TempFile) {
            let mut temporary = file_system
                .create_temp_file(Default::default())
                .unwrap_or_else(|error| {
                    panic!("temp-file contract: create failed: {error}")
                });
            let source = temporary.path().clone();
            temporary.cleanup().unwrap_or_else(|error| {
                panic!("temp-file contract: cleanup failed: {error}")
            });
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
            self.assert_temp_persist(&mut temporary, &target, "temp-file");
            self.context.record_created(target);
        }
        if self.capable(FileSystemCapability::TempDirectory) {
            let mut temporary = file_system
                .create_temp_directory(Default::default())
                .unwrap_or_else(|error| {
                    panic!("temp-directory contract: create failed: {error}")
                });
            let source = temporary.path().clone();
            temporary.cleanup().unwrap_or_else(|error| {
                panic!("temp-directory contract: cleanup failed: {error}")
            });
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
            self.assert_temp_directory_persist(&mut temporary, &target);
            self.context.record_created(target);
        }
    }

    /// Checks structured filesystem error context and redaction behavior.
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
    fn path(&self, relative: &str) -> qubit_fs::Path {
        let relative = self.context.relative_name(relative);
        self.fixture.path(&relative).unwrap_or_else(|error| {
            panic!(
                "{} contract: fixture path failed: {error}",
                self.context.current_contract()
            )
        })
    }

    /// Returns whether the immutable snapshot declares a capability.
    fn capable(&self, capability: FileSystemCapability) -> bool {
        self.context
            .properties()
            .capabilities()
            .contains(capability)
    }

    /// Seeds a resource and makes support mandatory for the requested
    /// capability.
    fn required_seed(
        &self,
        relative: &str,
        bytes: &[u8],
        contract: &str,
    ) -> qubit_fs::Path {
        match self.seed(relative, bytes) {
            FixtureSupport::Supported(path) => path,
            FixtureSupport::Unsupported => panic!(
                "{contract} contract: advertised capability requires fixture.seed_file support"
            ),
        }
    }

    /// Delegates out-of-band resource preparation to the fixture.
    fn seed(
        &self,
        relative: &str,
        bytes: &[u8],
    ) -> FixtureSupport<qubit_fs::Path> {
        let relative = self.context.relative_name(relative);
        self.fixture
            .seed_file(&relative, bytes)
            .unwrap_or_else(|error| {
                panic!(
                    "{} contract: fixture seed failed: {error}",
                    self.context.current_contract()
                )
            })
    }

    /// Reads a provider-owned probe through the fixture and checks exact bytes.
    fn assert_bytes(
        &self,
        path: &qubit_fs::Path,
        expected: &[u8],
        message: &str,
    ) {
        match self.fixture.read_file(path).unwrap_or_else(|error| {
            panic!("copy contract: fixture observation failed: {error}")
        }) {
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
    fn assert_temp_persist(
        &self,
        temporary: &mut TempFile,
        target: &qubit_fs::Path,
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
    fn assert_temp_directory_persist(
        &self,
        temporary: &mut TempDirectory,
        target: &qubit_fs::Path,
    ) {
        let outcome = temporary
            .persist(
                target,
                PersistOptions {
                    atomicity: if self
                        .capable(FileSystemCapability::AtomicTempPersist)
                    {
                        AtomicityRequirement::Required
                    } else {
                        AtomicityRequirement::Preferred
                    },
                    ..PersistOptions::default()
                },
            )
            .unwrap_or_else(|failure| {
                panic!(
                    "temp-directory contract: persist failed: {}",
                    failure.error()
                )
            });
        assert_eq!(
            outcome.target, *target,
            "temp-directory contract: persist target mismatch"
        );
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

    /// Persists one temporary file using the strongest guarantee it advertises.
    fn assert_temp_file_persist_result(
        &self,
        temporary: &mut TempFile,
        target: &qubit_fs::Path,
        label: &str,
    ) {
        let outcome = temporary
            .persist(
                target,
                PersistOptions {
                    atomicity: if self
                        .capable(FileSystemCapability::AtomicTempPersist)
                    {
                        AtomicityRequirement::Required
                    } else {
                        AtomicityRequirement::Preferred
                    },
                    ..PersistOptions::default()
                },
            )
            .unwrap_or_else(|failure| {
                panic!("{label} contract: persist failed: {}", failure.error())
            });
        assert_eq!(
            outcome.target, *target,
            "{label} contract: persist target mismatch"
        );
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
    fn assert_error(
        &self,
        error: &qubit_fs::FsError,
        kind: FsErrorKind,
        operation: FsOperation,
        path: &qubit_fs::Path,
        target: Option<&qubit_fs::Path>,
    ) {
        assert_eq!(error.kind(), kind, "error contract: kind mismatch");
        assert_eq!(
            error.operation(),
            operation,
            "error contract: context mismatch"
        );
        assert_eq!(error.path(), Some(path), "error contract: path mismatch");
        assert_eq!(error.target(), target, "error contract: target mismatch");
    }
}
