// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Implements temporary resource contracts.

use super::*;

impl<'a> FileSystemContractSuite<'a> {
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
            let incompatible_parent = match self.context.properties().info().path_semantics() {
                PathSemantics::Hierarchical => Path::parse_literal("/temp-invalid-parent"),
                _ => Path::parse("/temp-invalid-parent"),
            }
            .expect("incompatible temporary parent should parse");
            let error = match file_system.create_temp_file(
                TempFileOptions::default().with_parent(Some(incompatible_parent.clone())),
            ) {
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
            let parent = self.path("temp-file-parent");
            if self.capable(FileSystemCapability::CreateDirectory) {
                file_system
                    .create_directory(&parent, CreateDirectoryOptions::default())
                    .expect("temp/file: parent creation failed");
                self.context.record_created(parent.clone());
            }
            let options = TempFileOptions::default()
                .with_parent(
                    self.capable(FileSystemCapability::CreateDirectory)
                        .then_some(parent.clone()),
                )
                .with_prefix("contract-file-".to_owned())
                .with_suffix(".tmp".to_owned());
            let mut temporary = file_system
                .create_temp_file(options)
                .expect("temp/file: create failed");
            let source = temporary.path().clone();
            assert!(
                source.as_str().contains("/contract-file-"),
                "temp/file: requested prefix was ignored"
            );
            assert!(
                source.as_str().ends_with(".tmp"),
                "temp/file: requested suffix was ignored"
            );
            temporary
                .cleanup()
                .expect("temp/file: cleanup failed");
            assert!(
                !file_system
                    .exists(&source)
                    .expect("temp/file: source exists failed"),
                "temp/file: cleanup retained source"
            );
            let mut temporary = file_system
                .create_temp_file(
                    TempFileOptions::default().with_parent(
                        self.capable(FileSystemCapability::CreateDirectory)
                            .then_some(parent),
                    ),
                )
                .expect("temp/file: persist setup failed");
            let target = self.path("temp-file-parent/persisted-file");
            self.context.record_created(target.clone());
            self.assert_temp_persist(&mut temporary, &target, "temp/file");
        } else {
            let error = file_system
                .create_temp_file(TempFileOptions::default())
                .expect_err("temp/file: unadvertised creation succeeded");
            self.assert_pathless_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::CreateTemp,
            );
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
        if self.capable(FileSystemCapability::TempDirectory) {
            let parent = self.path("temp-directory-parent");
            if self.capable(FileSystemCapability::CreateDirectory) {
                file_system
                    .create_directory(&parent, CreateDirectoryOptions::default())
                    .expect("temp/directory: parent creation failed");
                self.context.record_created(parent.clone());
            }
            let options = TempDirectoryOptions::default()
                .with_parent(
                    self.capable(FileSystemCapability::CreateDirectory)
                        .then_some(parent.clone()),
                )
                .with_prefix("contract-directory-".to_owned())
                .with_suffix(".tmp".to_owned());
            let mut temporary = file_system
                .create_temp_directory(options)
                .expect("temp/directory: create failed");
            let source = temporary.path().clone();
            assert!(
                source.as_str().contains("/contract-directory-"),
                "temp/directory: requested prefix was ignored"
            );
            assert!(
                source.as_str().ends_with(".tmp"),
                "temp/directory: requested suffix was ignored"
            );
            temporary
                .cleanup()
                .expect("temp/directory: cleanup failed");
            assert!(
                !file_system
                    .exists(&source)
                    .expect("temp/directory: source exists failed"),
                "temp/directory: cleanup retained source"
            );
            let mut temporary = file_system
                .create_temp_directory(
                    TempDirectoryOptions::default().with_parent(
                        self.capable(FileSystemCapability::CreateDirectory)
                            .then_some(parent),
                    ),
                )
                .expect("temp/directory: persist setup failed");
            let target = self.path("temp-directory-parent/persisted-directory");
            self.context.record_created(target.clone());
            self.assert_temp_directory_persist(&mut temporary, &target);
            if self.capable(FileSystemCapability::CreateDirectory) {
                self.assert_temp_directory_overwrite();
            }
        } else {
            let error = file_system
                .create_temp_directory(TempDirectoryOptions::default())
                .expect_err("temp/directory: unadvertised creation succeeded");
            self.assert_pathless_error(
                &error,
                FsErrorKind::UnsupportedCapability,
                FsOperation::CreateTemp,
            );
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
    pub fn assert_temp_persist(&self, temporary: &mut TempFile, target: &Path, label: &str) {
        self.assert_temp_file_persist_result(temporary, target, label);
        if !self.capable(FileSystemCapability::AtomicTempPersist) {
            let mut retry = self
                .fixture
                .file_system()
                .create_temp_file(Default::default())
                .expect("temp/file: atomic preflight setup failed");
            let source = retry.path().clone();
            let failure = retry
                .persist(
                    &self.path("temp-required-atomic-file"),
                    PersistOptions::default(),
                )
                .expect_err("temp/file: unadvertised required atomic persist succeeded");
            assert_eq!(
                failure.state(),
                PersistFailureState::NotPublished,
                "temp/file: failed preflight changed publication responsibility"
            );
            assert_eq!(
                failure.error().kind(),
                FsErrorKind::RequirementNotMet,
                "temp/file: failed preflight kind mismatch"
            );
            assert_eq!(
                failure.error().operation(),
                FsOperation::PersistTemp,
                "temp/file: failed preflight operation mismatch"
            );
            assert_eq!(
                failure.error().path(),
                Some(&source),
                "temp/file: failed preflight source mismatch"
            );
            assert_eq!(
                failure.error().target(),
                Some(&self.path("temp-required-atomic-file")),
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
                    .expect("temp/file: source exists failed"),
                "temp/file: required atomic preflight removed source"
            );
            retry
                .cleanup()
                .expect("temp/file: retained source cleanup failed");
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
    pub fn assert_temp_directory_persist(
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
            .expect("temp/directory: persist failed");
        assert_eq!(
            outcome.target(),
            target,
            "temp/directory: persist target mismatch"
        );
        if self.capable(FileSystemCapability::AtomicTempPersist) {
            assert_eq!(
                outcome.atomicity(),
                AchievedAtomicity::Atomic,
                "temp/directory: required operation reported non-atomic publication"
            );
        }
        assert!(
            self.fixture
                .file_system()
                .exists(target)
                .expect("temp/directory: target exists failed"),
            "temp/directory: persist did not publish target"
        );
        if !self.capable(FileSystemCapability::AtomicTempPersist) {
            let mut retry = self
                .fixture
                .file_system()
                .create_temp_directory(Default::default())
                .expect("temp/directory: atomic preflight setup failed");
            let source = retry.path().clone();
            let failure = retry
                .persist(
                    &self.path("temp-required-atomic-directory"),
                    PersistOptions::default(),
                )
                .expect_err(
                    "temp/directory: unadvertised required atomic persist succeeded",
                );
            assert_eq!(
                failure.state(),
                PersistFailureState::NotPublished,
                "temp/directory: failed preflight changed publication responsibility"
            );
            assert_eq!(
                failure.error().kind(),
                FsErrorKind::RequirementNotMet,
                "temp/directory: failed preflight kind mismatch"
            );
            assert_eq!(
                failure.error().operation(),
                FsOperation::PersistTemp,
                "temp/directory: failed preflight operation mismatch"
            );
            assert_eq!(
                failure.error().path(),
                Some(&source),
                "temp/directory: failed preflight source mismatch"
            );
            assert_eq!(
                failure.error().target(),
                Some(&self.path("temp-required-atomic-directory")),
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
                    .expect("temp/directory: source exists failed"),
                "temp/directory: required atomic preflight removed source"
            );
            retry
                .cleanup()
                .expect("temp/directory: retained source cleanup failed");
        }
    }

    /// Verifies a temporary directory replaces an existing empty directory
    /// when the caller explicitly allows replacement.
    ///
    /// # Panics
    ///
    /// Panics when setup, overwrite publication, outcome reporting, or target
    /// observation violates the temporary-directory contract.
    pub fn assert_temp_directory_overwrite(&mut self) {
        let file_system = self.fixture.file_system();
        let parent = self.path("temp-overwrite-parent");
        self.context.record_created(parent.clone());
        file_system
            .create_directory(&parent, CreateDirectoryOptions::default())
            .expect("temp/directory: parent setup failed");
        let target = self.path("temp-overwrite-parent/temp-overwritten-directory");
        self.context.record_created(target.clone());
        file_system
            .create_directory(&target, CreateDirectoryOptions::default())
            .expect("temp/directory: destination setup failed");
        let mut temporary = file_system
            .create_temp_directory(TempDirectoryOptions::default().with_parent(Some(parent)))
            .expect("temp/directory: temporary creation failed");
        let outcome = temporary
            .persist(
                &target,
                PersistOptions::default()
                    .with_overwrite(true)
                    .with_atomicity(if self.capable(FileSystemCapability::AtomicTempPersist) {
                        AtomicityRequirement::Required
                    } else {
                        AtomicityRequirement::Preferred
                    }),
            )
            .expect("temp/directory: persist failed");
        assert_eq!(
            outcome.target(),
            &target,
            "temp/directory: persist target mismatch"
        );
        assert!(
            file_system
                .exists(&target)
                .expect("temp/directory: target exists failed"),
            "temp/directory: replacement did not publish target"
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
    pub fn assert_temp_file_persist_result(
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
            .unwrap_or_else(|error| panic!("{label}: persist failed [temp/atomic]: {error}"));
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
                .expect("temp/file: target exists failed"),
            "{label} contract: persist did not publish target"
        );
    }
}
