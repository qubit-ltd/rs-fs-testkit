// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements temporary resource contracts.

use super::*;

impl<'a> AsyncFileSystemContractSuite<'a> {
    /// Checks asynchronous temporary-resource lifecycle behavior.
    ///
    /// # Panics
    ///
    /// Panics when temporary resource options, cleanup, or publication violates
    /// an advertised capability.
    pub async fn assert_temp_resources(&mut self) {
        self.context.begin("temp_resources");
        if self.capable(FileSystemCapability::TempFile) {
            let incompatible_parent = match self.context.properties().info().path_semantics() {
                PathSemantics::Hierarchical => Path::parse_literal("/async-temp-invalid-parent"),
                _ => Path::parse("/async-temp-invalid-parent"),
            }
            .expect("incompatible temporary parent should parse");
            let error = match self
                .fixture
                .file_system()
                .create_temp_file(TempFileOptions::default().with_parent(Some(incompatible_parent.clone())))
                .await
            {
                Ok(_) => panic!("temp/file: invalid parent succeeded"),
                Err(error) => error,
            };
            assert_eq!(
                error.kind(),
                FsErrorKind::InvalidPath,
                "temp/file: parent validation kind mismatch"
            );
            assert_eq!(
                error.operation(),
                FsOperation::CreateTemp,
                "temp/file: parent validation operation mismatch"
            );
            assert_eq!(
                error.path(),
                Some(&incompatible_parent),
                "temp/file: parent validation path mismatch"
            );
            self.assert_temp_file_options().await;
            let mut temporary = self
                .fixture
                .file_system()
                .create_temp_file(TempFileOptions::default())
                .await
                .expect("temp/file: advertised creation failed");
            let path = temporary.path().clone();
            temporary.cleanup().await.expect("temp/file: cleanup failed");
            let error = self
                .fixture
                .file_system()
                .stat(&path)
                .await
                .expect_err("temp/file: cleanup retained source");
            self.assert_error(&error, FsErrorKind::NotFound, FsOperation::Stat, &path);
            let mut kept = self
                .fixture
                .file_system()
                .create_temp_file(TempFileOptions::default())
                .await
                .expect("temp/file: keep setup failed");
            let kept_source = kept.path().clone();
            self.context.record_created(kept_source.clone());
            // Keep the provider-owned temporary entry intact. Writing via
            // the facade may atomically replace its path and invalidate the
            // lifecycle handle's native identity before `keep` is called.
            let kept_outcome = kept.keep().await.expect("temp/file: keep failed");
            self.context.record_created(kept_outcome.target().clone());
            assert_eq!(kept.state(), TempResourceState::Kept, "temp/file: keep state mismatch");
            assert_ne!(
                kept_outcome.target(),
                &kept_source,
                "temp/file: keep reused source identity"
            );
            assert!(
                !self
                    .fixture
                    .file_system()
                    .exists(&kept_source)
                    .await
                    .expect("temp/file: kept source exists failed")
            );
            assert!(
                self.fixture
                    .file_system()
                    .exists(kept_outcome.target())
                    .await
                    .expect("temp/file: kept target exists failed")
            );
            let mut temporary = self
                .fixture
                .file_system()
                .create_temp_file(TempFileOptions::default())
                .await
                .expect("temp/file: persist setup failed");
            let target = self.path("async-temp-persisted-file");
            self.context.record_created(target.clone());
            self.assert_temp_file_persist(&mut temporary, &target).await;
        } else {
            let error = match self
                .fixture
                .file_system()
                .create_temp_file(TempFileOptions::default())
                .await
            {
                Ok(_) => panic!("temp/file: unadvertised creation succeeded"),
                Err(error) => error,
            };
            self.assert_pathless_error(&error, FsErrorKind::UnsupportedCapability, FsOperation::CreateTemp);
        }
        if self.capable(FileSystemCapability::TempDirectory) {
            self.assert_temp_directory_options().await;
            let mut temporary = self
                .fixture
                .file_system()
                .create_temp_directory(TempDirectoryOptions::default())
                .await
                .expect("temp/directory: advertised creation failed");
            let path = temporary.path().clone();
            temporary.cleanup().await.expect("temp/directory: cleanup failed");
            let error = self
                .fixture
                .file_system()
                .stat(&path)
                .await
                .expect_err("temp/directory: cleanup retained source");
            self.assert_error(&error, FsErrorKind::NotFound, FsOperation::Stat, &path);
            let mut temporary = self
                .fixture
                .file_system()
                .create_temp_directory(TempDirectoryOptions::default())
                .await
                .expect("temp/directory: persist setup failed");
            let target = self.path("async-temp-persisted-directory");
            self.context.record_created(target.clone());
            self.assert_temp_directory_persist(&mut temporary, &target).await;
            if self.capable(FileSystemCapability::CreateDirectory) {
                self.assert_temp_directory_overwrite().await;
            }
        } else {
            let error = match self
                .fixture
                .file_system()
                .create_temp_directory(TempDirectoryOptions::default())
                .await
            {
                Ok(_) => panic!("temp/directory: unadvertised creation succeeded"),
                Err(error) => error,
            };
            self.assert_pathless_error(&error, FsErrorKind::UnsupportedCapability, FsOperation::CreateTemp);
        }
        self.context.record_check(
            "temp/file",
            Some(FileSystemCapability::TempFile),
            if self.capable(FileSystemCapability::TempFile) {
                ContractCheckOutcome::Passed
            } else {
                ContractCheckOutcome::RejectedAsExpected
            },
        );
        self.context.record_check(
            "temp/directory",
            Some(FileSystemCapability::TempDirectory),
            if self.capable(FileSystemCapability::TempDirectory) {
                ContractCheckOutcome::Passed
            } else {
                ContractCheckOutcome::RejectedAsExpected
            },
        );
        self.context.record_check(
            "temp/atomic",
            Some(FileSystemCapability::AtomicTempPersist),
            if self.capable(FileSystemCapability::AtomicTempPersist) {
                ContractCheckOutcome::Passed
            } else {
                ContractCheckOutcome::RejectedAsExpected
            },
        );
    }

    /// Verifies asynchronous temporary-file persistence publication.
    pub async fn assert_temp_file_persist(&self, temporary: &mut AsyncTempFile, target: &Path) {
        let outcome = temporary
            .persist(target, self.temp_persist_options())
            .await
            .unwrap_or_else(|error| panic!("temp/file: persist failed [temp/atomic]: {error}"));
        self.assert_temp_persist_outcome(&outcome, target, "temp/file").await;
        if !self.capable(FileSystemCapability::AtomicTempPersist) {
            let mut retry = self
                .fixture
                .file_system()
                .create_temp_file(TempFileOptions::default())
                .await
                .expect("temp/file: atomic preflight setup failed");
            let source = retry.path().clone();
            let failure = retry
                .persist(&self.path("async-temp-required-atomic-file"), PersistOptions::default())
                .await
                .expect_err("temp/file: unadvertised required atomic persist succeeded");
            assert_eq!(
                failure.state(),
                PersistFailureState::NotPublished,
                "temp/file: failed preflight changed publication responsibility"
            );
            self.assert_requirement_error(
                failure.error(),
                FsOperation::PersistTemp,
                FileSystemCapability::AtomicTempPersist,
                "temp/file",
            );
            assert_eq!(
                failure.error().path(),
                Some(&source),
                "temp/file: failed preflight source mismatch"
            );
            assert_eq!(
                failure.error().target(),
                Some(&self.path("async-temp-required-atomic-file")),
                "temp/file: failed preflight target mismatch"
            );
            assert_eq!(
                failure.error().provider(),
                Some(self.context.properties().info().provider_id()),
                "temp/file: failed preflight provider mismatch"
            );
            assert!(
                self.fixture
                    .file_system()
                    .exists(&source)
                    .await
                    .expect("temp/file: source exists failed"),
                "temp/file: required atomic preflight removed source"
            );
            retry
                .cleanup()
                .await
                .expect("temp/file: retained source cleanup failed");
        }
    }

    /// Verifies asynchronous temporary-directory persistence publication.
    pub async fn assert_temp_directory_persist(&self, temporary: &mut AsyncTempDirectory, target: &Path) {
        let outcome = temporary
            .persist(target, self.temp_persist_options())
            .await
            .expect("temp/directory: persist failed");
        self.assert_temp_persist_outcome(&outcome, target, "temp-directory contract")
            .await;
        if !self.capable(FileSystemCapability::AtomicTempPersist) {
            let mut retry = self
                .fixture
                .file_system()
                .create_temp_directory(TempDirectoryOptions::default())
                .await
                .expect("temp/directory: atomic preflight setup failed");
            let source = retry.path().clone();
            let failure = retry
                .persist(
                    &self.path("async-temp-required-atomic-directory"),
                    PersistOptions::default(),
                )
                .await
                .expect_err("temp/directory: unadvertised required atomic persist succeeded");
            assert_eq!(
                failure.state(),
                PersistFailureState::NotPublished,
                "temp/directory: failed preflight changed publication responsibility"
            );
            self.assert_requirement_error(
                failure.error(),
                FsOperation::PersistTemp,
                FileSystemCapability::AtomicTempPersist,
                "temp-directory contract",
            );
            assert_eq!(
                failure.error().path(),
                Some(&source),
                "temp/directory: failed preflight source mismatch"
            );
            assert_eq!(
                failure.error().target(),
                Some(&self.path("async-temp-required-atomic-directory")),
                "temp/directory: failed preflight target mismatch"
            );
            assert_eq!(
                failure.error().provider(),
                Some(self.context.properties().info().provider_id()),
                "temp/directory: failed preflight provider mismatch"
            );
            assert!(
                self.fixture
                    .file_system()
                    .exists(&source)
                    .await
                    .expect("temp/directory: source exists failed"),
                "temp/directory: required atomic preflight removed source"
            );
            retry
                .cleanup()
                .await
                .expect("temp/directory: retained source cleanup failed");
        }
    }

    /// Verifies asynchronous overwrite publication for an empty directory.
    pub async fn assert_temp_directory_overwrite(&mut self) {
        let target = self.path("async-temp-overwritten-directory");
        self.context.record_created(target.clone());
        self.fixture
            .file_system()
            .create_directory(&target, CreateDirectoryOptions::default())
            .await
            .expect("temp/directory: destination setup failed");
        let mut temporary = self
            .fixture
            .file_system()
            .create_temp_directory(TempDirectoryOptions::default())
            .await
            .expect("temp/directory: temporary creation failed");
        let outcome = temporary
            .persist(&target, self.temp_persist_options().with_overwrite(true))
            .await
            .expect("temp/directory: persist failed");
        assert_eq!(outcome.target(), &target);
        assert!(
            self.fixture
                .file_system()
                .exists(&target)
                .await
                .expect("temp/directory: target observation failed")
        );
    }

    /// Checks asynchronous temporary-file parent and affix options.
    ///
    /// # Panics
    ///
    /// Panics when a temporary file ignores the requested parent, prefix, or
    /// suffix, or when cleanup fails.
    pub async fn assert_temp_file_options(&mut self) {
        let parent = self.prepare_temp_options_parent("async-temp-file-options-parent").await;
        let mut temporary = self
            .fixture
            .file_system()
            .create_temp_file(
                TempFileOptions::default()
                    .with_parent(parent.clone())
                    .with_prefix("async-file-".to_owned())
                    .with_suffix(".tmp".to_owned()),
            )
            .await
            .expect("temp/file: option-aware creation failed");
        self.assert_temp_path(temporary.path(), parent.as_ref(), "async-file-", ".tmp", "temp/file");
        temporary
            .cleanup()
            .await
            .expect("temp/file: option-aware cleanup failed");
    }

    /// Checks asynchronous temporary-directory parent and affix options.
    ///
    /// # Panics
    ///
    /// Panics when a temporary directory ignores the requested parent, prefix,
    /// or suffix, or when cleanup fails.
    pub async fn assert_temp_directory_options(&mut self) {
        let parent = self
            .prepare_temp_options_parent("async-temp-directory-options-parent")
            .await;
        let mut temporary = self
            .fixture
            .file_system()
            .create_temp_directory(
                TempDirectoryOptions::default()
                    .with_parent(parent.clone())
                    .with_prefix("async-directory-".to_owned())
                    .with_suffix(".tmpdir".to_owned()),
            )
            .await
            .expect("temp/directory: option-aware creation failed");
        self.assert_temp_path(
            temporary.path(),
            parent.as_ref(),
            "async-directory-",
            ".tmpdir",
            "temp-directory contract",
        );
        temporary
            .cleanup()
            .await
            .expect("temp/directory: option-aware cleanup failed");
    }

    /// Prepares the parent directory used by temporary option assertions.
    ///
    /// # Returns
    ///
    /// The created parent path when directory creation is advertised, or `None`
    /// otherwise.
    ///
    /// # Panics
    ///
    /// Panics when an advertised directory creation operation fails.
    pub async fn prepare_temp_options_parent(&mut self, relative: &str) -> Option<Path> {
        if !self.capable(FileSystemCapability::CreateDirectory) {
            return None;
        }
        let parent = self.path(relative);
        self.fixture
            .file_system()
            .create_directory(&parent, CreateDirectoryOptions::default())
            .await
            .expect("temporary resource contract: option parent creation failed");
        self.context.record_created(parent.clone());
        Some(parent)
    }

    /// Checks that a temporary resource honors its requested location and name.
    ///
    /// # Parameters
    ///
    /// * `path` - Provider-created temporary resource path.
    /// * `parent` - Requested parent path, when one was requested.
    /// * `prefix` - Requested filename prefix.
    /// * `suffix` - Requested filename suffix.
    /// * `contract` - Contract label used in diagnostics.
    ///
    /// # Panics
    ///
    /// Panics when the path is not an immediate child of an explicitly
    /// requested `parent`, or does not honor the requested affixes.
    pub fn assert_temp_path(&self, path: &Path, parent: Option<&Path>, prefix: &str, suffix: &str, contract: &str) {
        let name = path
            .as_str()
            .rsplit('/')
            .next()
            .expect("temporary resource path must have a final component");
        if let Some(parent) = parent {
            let parent_prefix = format!("{}/", parent.as_str().trim_end_matches('/'));
            assert!(
                path.as_str()
                    .strip_prefix(&parent_prefix)
                    .is_some_and(|relative| !relative.contains('/'),),
                "{contract}: temporary resource was not an immediate child of the requested parent"
            );
        }
        assert!(
            name.starts_with(prefix),
            "{contract}: temporary resource prefix mismatch"
        );
        assert!(name.ends_with(suffix), "{contract}: temporary resource suffix mismatch");
    }

    /// Builds persistence options matching the advertised atomic guarantee.
    #[inline]
    pub fn temp_persist_options(&self) -> PersistOptions {
        PersistOptions::default().with_atomicity(if self.capable(FileSystemCapability::AtomicTempPersist) {
            AtomicityRequirement::Required
        } else {
            AtomicityRequirement::Preferred
        })
    }

    /// Checks persistence reporting and destination publication.
    pub async fn assert_temp_persist_outcome(&self, outcome: &PersistOutcome, target: &Path, label: &str) {
        assert_eq!(outcome.target(), target, "{label}: persist target mismatch");
        if self.capable(FileSystemCapability::AtomicTempPersist) {
            assert_eq!(
                outcome.atomicity(),
                AchievedAtomicity::Atomic,
                "{label}: required operation reported non-atomic publication"
            );
        }
        assert!(
            self.fixture
                .file_system()
                .exists(target)
                .await
                .expect("temporary resource contract: target exists failed"),
            "{label}: persist did not publish target"
        );
    }
}
