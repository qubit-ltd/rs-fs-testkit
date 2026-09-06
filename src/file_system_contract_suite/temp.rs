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
                    .create_directory(&parent, CreateDirectoryOptions::default())
                    .expect("temp-file contract: parent creation failed");
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
                .create_temp_file(
                    TempFileOptions::default().with_parent(
                        self.capable(FileSystemCapability::CreateDirectory)
                            .then_some(parent),
                    ),
                )
                .expect("temp-file contract: persist setup failed");
            let target = self.path("temp-file-parent/persisted-file");
            self.context.record_created(target.clone());
            self.assert_temp_persist(&mut temporary, &target, "temp-file");
        } else {
            let error = file_system
                .create_temp_file(TempFileOptions::default())
                .expect_err("temp-file contract: unadvertised creation succeeded");
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
                    .create_directory(&parent, CreateDirectoryOptions::default())
                    .expect("temp-directory contract: parent creation failed");
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
                .create_temp_directory(
                    TempDirectoryOptions::default().with_parent(
                        self.capable(FileSystemCapability::CreateDirectory)
                            .then_some(parent),
                    ),
                )
                .expect("temp-directory contract: persist setup failed");
            let target = self.path("temp-directory-parent/persisted-directory");
            self.context.record_created(target.clone());
            self.assert_temp_directory_persist(&mut temporary, &target);
            if self.capable(FileSystemCapability::CreateDirectory) {
                self.assert_temp_directory_overwrite();
            }
        } else {
            let error = file_system
                .create_temp_directory(TempDirectoryOptions::default())
                .expect_err("temp-directory contract: unadvertised creation succeeded");
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
                .expect("temp-directory contract: atomic preflight setup failed");
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
            retry
                .cleanup()
                .expect("temp-directory contract: retained source cleanup failed");
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
            .expect("temp-directory overwrite contract: parent setup failed");
        let target = self.path("temp-overwrite-parent/temp-overwritten-directory");
        self.context.record_created(target.clone());
        file_system
            .create_directory(&target, CreateDirectoryOptions::default())
            .expect("temp-directory overwrite contract: destination setup failed");
        let mut temporary = file_system
            .create_temp_directory(TempDirectoryOptions::default().with_parent(Some(parent)))
            .expect("temp-directory overwrite contract: temporary creation failed");
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
            .expect("temp-directory overwrite contract: persist failed");
        assert_eq!(
            outcome.target(),
            &target,
            "temp-directory overwrite contract: persist target mismatch"
        );
        assert!(
            file_system
                .exists(&target)
                .expect("temp-directory overwrite contract: target exists failed"),
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
}
