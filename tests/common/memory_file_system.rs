// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! A deliberately small SPI-backed provider used to self-test contract suites.

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::Cursor;
use std::io::Result as IoResult;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_fs as qfs;
use qubit_fs::FileSystem;
use qubit_fs::copy::CopyConflictPolicy;
use qubit_fs::copy::CopyFailureState;
use qubit_fs::copy::CopyMethod;
use qubit_fs::copy::CopyMode;
use qubit_fs::copy::CopyOptions;
use qubit_fs::copy::CopyOutcome;
use qubit_fs::copy::CopyStats;
use qubit_fs::copy::ServerSidePreference;
use qubit_fs::directory::CreateDirectoryOutcome;
use qubit_fs::directory::DeleteOutcome;
use qubit_fs::directory::ListOptions;
use qubit_fs::error::FsError;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::error::FsResult;
use qubit_fs::metadata::AchievedAtomicity;
use qubit_fs::metadata::AtomicityRequirement;
use qubit_fs::metadata::DirEntry;
use qubit_fs::metadata::DurabilityRequirement;
use qubit_fs::metadata::FileKind;
use qubit_fs::metadata::FileMetadata;
use qubit_fs::metadata::FileSystemCapabilities;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::FileSystemId;
use qubit_fs::metadata::FileSystemInfo;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs::metadata::FileSystemProperties;
use qubit_fs::metadata::OpenedFileInfo;
use qubit_fs::metadata::PublicationMethod;
use qubit_fs::metadata::ResourceVersion;
use qubit_fs::metadata::SymlinkPolicy;
use qubit_fs::metadata::WriteOutcome;
use qubit_fs::path::Path;
use qubit_fs::path::PathConstraints;
use qubit_fs::path::PathSemantics;
use qubit_fs::read::ChecksumPolicy;
use qubit_fs::rename::RenameFailureState;
use qubit_fs::rename::RenameOutcome;
use qubit_fs::spi::CopyAttempt;
use qubit_fs::spi::CopyDeclineReason;
use qubit_fs::spi::CopyRequest;
use qubit_fs::spi::CreateDirectoryRequest;
use qubit_fs::spi::CreateTempDirectoryRequest;
use qubit_fs::spi::CreateTempFileRequest;
use qubit_fs::spi::DeleteDirectoryRequest;
use qubit_fs::spi::DeleteFileRequest;
use qubit_fs::spi::DirectoryStreamSpi;
use qubit_fs::spi::FileSystemSpi;
use qubit_fs::spi::FileWriterSpi;
use qubit_fs::spi::ListRequest;
use qubit_fs::spi::OpenReaderRequest;
use qubit_fs::spi::OpenWriterRequest;
use qubit_fs::spi::OpenedDirectoryStream;
use qubit_fs::spi::OpenedReader;
use qubit_fs::spi::OpenedTempDirectory;
use qubit_fs::spi::OpenedTempFile;
use qubit_fs::spi::OpenedWriter;
use qubit_fs::spi::PersistRequest;
use qubit_fs::spi::ProviderOperation;
use qubit_fs::spi::ProviderOperations;
use qubit_fs::spi::ProviderProperties;
use qubit_fs::spi::RenameRequest;
use qubit_fs::spi::SpiCopyFailure;
use qubit_fs::spi::SpiPersistFailure;
use qubit_fs::spi::SpiRenameFailure;
use qubit_fs::spi::SpiWriteFailure;
use qubit_fs::spi::StatRequest;
use qubit_fs::spi::StatResponse;
use qubit_fs::spi::TempResourceSpi;
use qubit_fs::temp::PersistOutcome;
use qubit_fs::write::WriteAbortOutcome;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteFailureState;
use qubit_fs::write::WriteOptions;
use qubit_fs::write::WritePrecondition;
use qubit_fs_testkit as testkit;
use qubit_fs_testkit::CopyFixtureCase;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;
use qubit_io::Output;

use super::shared_model::Entry;
use crate::common::UnavailableScenario;

struct State {
    entries: HashMap<String, Entry>,
    versions: HashMap<String, u64>,
    next_temp: u64,
    fault: MemoryFault,
    delete_capability: bool,
    core_capabilities: bool,
    optional_capabilities: bool,
    create_directory_capability: bool,
    extended_capabilities: bool,
    native_copy: bool,
    fallback_only: bool,
    delete_attempts: usize,
    limits: FileSystemLimits,
    unavailable_case: Option<UnavailableScenario>,
    read_only: bool,
}

pub(crate) fn provider_properties(properties: FileSystemProperties) -> ProviderProperties {
    provider_properties_with_copy(properties, true)
}

fn provider_properties_with_copy(properties: FileSystemProperties, try_copy: bool) -> ProviderProperties {
    let mut operations = ProviderOperations::new()
        .with(ProviderOperation::Stat)
        .with(ProviderOperation::List)
        .with(ProviderOperation::OpenReader)
        .with(ProviderOperation::OpenWriter)
        .with(ProviderOperation::CreateDirectory)
        .with(ProviderOperation::DeleteFile)
        .with(ProviderOperation::DeleteDirectory);
    if try_copy {
        operations = operations.with(ProviderOperation::TryCopy);
    }
    ProviderProperties::new(
        properties.info().clone(),
        operations
            .with(ProviderOperation::Rename)
            .with(ProviderOperation::CreateTempFile)
            .with(ProviderOperation::CreateTempDirectory),
        properties.capabilities(),
        *properties.limits(),
        properties.path_constraints().clone(),
        properties.symlink_policy(),
    )
    .expect("memory provider properties must be valid")
}

fn keep_target(source: &Path) -> Path {
    Path::parse(&format!("/kept{}", source.as_str())).expect("generated keep target must be valid")
}

fn publish_entry(state: &mut State, path: &str, entry: Entry) {
    let version = state.versions.get(path).copied().unwrap_or(0).saturating_add(1);
    state.entries.insert(path.to_owned(), entry);
    state.versions.insert(path.to_owned(), version);
}

fn remove_entry(state: &mut State, path: &str) -> Option<Entry> {
    let removed = state.entries.remove(path);
    if removed.is_some() {
        state.versions.remove(path);
    }
    removed
}

/// One isolated contract fixture backed by the public synchronous facade.
pub struct MemoryFixture {
    file_system: FileSystem,
    state: Arc<Mutex<State>>,
    path_calls: Arc<AtomicUsize>,
}

/// A single injected provider defect used by the self-test matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryFault {
    /// Behaves conformingly.
    None,
    /// Returns directory metadata for an existing file.
    WrongStatKind,
    /// Claims recursive creation while omitting ancestors.
    RecursiveCreateLeavesParentsMissing,
    /// Leaves a cleaned temporary resource in the namespace.
    KeepTempOnCleanup,
    /// Reports a persisted target different from the requested target.
    WrongPersistTarget,
    /// Returns no entries for a non-empty requested directory.
    EmptyList,
    /// Returns bytes different from the provider's stored content.
    ReadWrongBytes,
    /// Accepts writes without publishing their content.
    WriteDropsBytes,
    /// Rejects commit while retaining the acquired writer for recovery.
    WriteCommitFailure,
    /// Reports deletion success without removing the resource.
    DeleteNoOp,
    /// Reports rename success without moving the resource.
    RenameNoOp,
    /// Omits eagerly requested listing metadata.
    ListDropsMetadata,
    /// Copies a directory root without its descendants.
    DirectoryCopyDropsChildren,
    /// Reports successful overwrite while retaining the old destination bytes.
    CopyOverwriteKeepsTarget,
    /// Ignores temporary-resource parent and affix options.
    TempIgnoresOptions,
    /// Uses object and prefix metadata kinds for stored resources.
    ObjectKinds,
    /// Overwrites instead of appending to an existing file.
    AppendOverwrites,
    /// Ignores CreateNew and replaces an existing destination.
    CreateNewOverwrites,
    /// Replaces only a prefix and leaves the old file suffix intact.
    ReplaceKeepsSuffix,
    /// Fails the first explicit abort while retaining a recoverable writer.
    AbortFailsOnce,
    /// Publishes during explicit abort while claiming the target is unchanged.
    AbortLies,
    /// Reports non-atomic completion for an atomic-required rename.
    AtomicRenameNonAtomic,
    /// Reports non-atomic completion for an atomic-required write.
    AtomicReplaceNonAtomic,
    /// Reports a non-durable completion for a durability-required copy.
    DurableFileCopyNonDurable,
    /// Publishes a file copy without the required atomic guarantee.
    AtomicFileCopyNonAtomic,
    /// Publishes a tree without its required atomic guarantee.
    AtomicTreeCopyNonAtomic,
    /// Publishes a tree without its required durability guarantee.
    DurableTreeCopyNonDurable,
    /// Copies the complete tree but omits descendant byte statistics.
    TreeCopyWrongStats,
    /// Reports a non-durable completion for a durability-required rename.
    DurableRenameNonDurable,
    /// Reports non-atomic completion for an atomic-required temp persist.
    AtomicTempPersistNonAtomic,
    /// Reports a non-server-side completion for a server-side-required copy.
    ServerSideCopyFallsBack,
    /// Removes the requested directory but leaves recursive descendants.
    RecursiveDeleteLeavesChildren,
    /// Ignores a stale If-Match read precondition.
    IgnoreReadIfMatch,
    /// Ignores an If-None-Match read precondition.
    IgnoreReadIfNoneMatch,
    /// Ignores an If-Match write precondition.
    IgnoreWriteIfMatch,
    /// Ignores a stale If-Match delete precondition.
    IgnoreDeleteIfMatch,
    /// Reports atomic replacement while retaining the old bytes.
    AtomicReplaceKeepsOldBytes,
    /// Reports durable write completion while dropping its bytes.
    DurableWriteDropsBytes,
    /// Reports checksum validation while returning corrupted bytes.
    ChecksumIgnoresCorruption,
    /// Returns an ordinary error while cleanup inspects a resource.
    CleanupStatError,
    /// Returns an ordinary error while cleanup deletes a resource.
    CleanupDeleteError,
    /// Panics while cleanup deletes a resource.
    CleanupDeletePanic,
}

impl MemoryFixture {
    /// Creates a fresh conforming fixture.
    pub fn new() -> Self {
        Self::with_fault(MemoryFault::None)
    }

    /// Creates a conforming fixture whose copy primitive completes natively.
    pub fn with_native_copy() -> Self {
        let fixture = Self::new();
        fixture
            .state
            .lock()
            .expect("memory state lock must succeed")
            .native_copy = true;
        fixture
    }

    /// Creates a provider whose copy implementation is only the facade
    /// stream fallback.
    pub fn fallback_only() -> Self {
        Self::with_configuration(
            MemoryFault::None,
            true,
            true,
            false,
            true,
            false,
            "memory-fallback-provider",
        )
    }

    /// Creates a fresh fixture with exactly one provider fault.
    pub fn with_fault(fault: MemoryFault) -> Self {
        let extended = matches!(
            fault,
            MemoryFault::DirectoryCopyDropsChildren
                | MemoryFault::AtomicRenameNonAtomic
                | MemoryFault::AtomicReplaceNonAtomic
                | MemoryFault::DurableFileCopyNonDurable
                | MemoryFault::AtomicFileCopyNonAtomic
                | MemoryFault::AtomicTreeCopyNonAtomic
                | MemoryFault::DurableTreeCopyNonDurable
                | MemoryFault::TreeCopyWrongStats
                | MemoryFault::DurableRenameNonDurable
                | MemoryFault::AtomicTempPersistNonAtomic
                | MemoryFault::ServerSideCopyFallsBack
                | MemoryFault::DurableWriteDropsBytes
                | MemoryFault::IgnoreReadIfMatch
                | MemoryFault::IgnoreReadIfNoneMatch
                | MemoryFault::IgnoreWriteIfMatch
                | MemoryFault::IgnoreDeleteIfMatch
                | MemoryFault::ChecksumIgnoresCorruption
        );
        let fixture = Self::with_configuration(fault, true, true, true, true, extended, "memory-contract-provider");
        if matches!(
            fault,
            MemoryFault::DirectoryCopyDropsChildren
                | MemoryFault::ServerSideCopyFallsBack
                | MemoryFault::DurableFileCopyNonDurable
                | MemoryFault::AtomicFileCopyNonAtomic
                | MemoryFault::AtomicTreeCopyNonAtomic
                | MemoryFault::DurableTreeCopyNonDurable
                | MemoryFault::TreeCopyWrongStats
        ) {
            fixture
                .state
                .lock()
                .expect("memory state lock must succeed")
                .native_copy = true;
        }
        fixture
    }

    /// Creates a fixture with no write primitive in its advertised profile.
    pub fn read_only() -> Self {
        Self::with_configuration(
            MemoryFault::None,
            true,
            true,
            false,
            true,
            false,
            "memory-read-only-provider",
        )
    }

    /// Creates a fixture exposing a concrete provider limit snapshot.
    pub fn with_limits(limits: FileSystemLimits) -> Self {
        Self::with_configuration_and_limits(
            MemoryFault::None,
            true,
            true,
            true,
            true,
            true,
            "memory-limited-provider",
            limits,
        )
    }

    /// Creates a fixture which cannot prepare one conditional case.
    pub fn with_conditional_case_unavailable(case: UnavailableScenario) -> Self {
        let fixture = Self::with_configuration(
            MemoryFault::None,
            true,
            true,
            true,
            true,
            true,
            "memory-conditional-case-provider",
        );
        fixture
            .state
            .lock()
            .expect("memory state lock must succeed")
            .unavailable_case = Some(case);
        fixture
    }

    /// Creates a fixture whose facade does not advertise deletion.
    pub fn without_delete() -> Self {
        Self::with_capabilities(MemoryFault::None, false, true)
    }

    /// Creates a fixture without the core read, write, list, and copy
    /// capabilities.
    pub fn without_core_capabilities() -> Self {
        Self::with_capabilities(MemoryFault::None, true, false)
    }

    /// Creates a fixture that advertises none of the suite operation
    /// capabilities.
    pub fn without_operation_capabilities() -> Self {
        Self::with_configuration(
            MemoryFault::None,
            false,
            false,
            false,
            false,
            false,
            "memory-contract-provider",
        )
    }

    /// Creates a fixture that exposes only the core capabilities.
    pub fn without_optional_capabilities() -> Self {
        Self::with_configuration(
            MemoryFault::None,
            true,
            true,
            false,
            false,
            false,
            "memory-contract-provider",
        )
    }

    /// Creates a conforming fixture whose filesystem and provider identifiers
    /// are identical.
    pub fn with_matching_ids() -> Self {
        Self::with_configuration(MemoryFault::None, true, true, true, true, false, "memory-contract")
    }

    /// Creates a fixture using object and prefix metadata kinds.
    pub fn with_object_kinds() -> Self {
        Self::with_configuration(
            MemoryFault::ObjectKinds,
            true,
            true,
            true,
            true,
            false,
            "memory-object-provider",
        )
    }

    /// Creates a fixture supporting recursive prefix deletion without directory
    /// creation.
    pub fn recursive_delete_without_create_directory() -> Self {
        Self::with_configuration(
            MemoryFault::None,
            true,
            true,
            true,
            false,
            false,
            "memory-prefix-provider",
        )
    }

    /// Creates a fixture advertising every capability contract.
    pub fn tree_copy_without_directory_creation() -> Self {
        Self::with_configuration(MemoryFault::None, true, true, true, false, true, "memory-tree-provider")
    }

    pub fn with_all_capabilities() -> Self {
        Self::with_configuration(
            MemoryFault::None,
            true,
            true,
            true,
            true,
            true,
            "memory-all-capabilities-provider",
        )
    }

    /// Creates a fixture with one injected fault and deletion-capability value.
    ///
    /// `fault` selects the provider defect; `delete_capability` controls the
    /// facade property snapshot returned to the suite.
    fn with_capabilities(fault: MemoryFault, delete_capability: bool, core_capabilities: bool) -> Self {
        Self::with_configuration(
            fault,
            delete_capability,
            core_capabilities,
            true,
            true,
            false,
            "memory-contract-provider",
        )
    }

    /// Creates a fixture with selected capabilities and provider identity.
    fn with_configuration(
        fault: MemoryFault,
        delete_capability: bool,
        core_capabilities: bool,
        optional_capabilities: bool,
        create_directory_capability: bool,
        extended_capabilities: bool,
        provider_id: &'static str,
    ) -> Self {
        Self::with_configuration_and_limits(
            fault,
            delete_capability,
            core_capabilities,
            optional_capabilities,
            create_directory_capability,
            extended_capabilities,
            provider_id,
            FileSystemLimits::unknown(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn with_configuration_and_limits(
        fault: MemoryFault,
        delete_capability: bool,
        core_capabilities: bool,
        optional_capabilities: bool,
        create_directory_capability: bool,
        extended_capabilities: bool,
        provider_id: &'static str,
        limits: FileSystemLimits,
    ) -> Self {
        let state = Arc::new(Mutex::new(State {
            entries: HashMap::new(),
            versions: HashMap::new(),
            next_temp: 0,
            fault,
            delete_capability,
            core_capabilities,
            optional_capabilities,
            create_directory_capability,
            extended_capabilities,
            native_copy: false,
            fallback_only: provider_id == "memory-fallback-provider",
            delete_attempts: 0,
            limits,
            unavailable_case: None,
            read_only: provider_id == "memory-read-only-provider",
        }));
        let path_calls = Arc::new(AtomicUsize::new(0));
        let file_system = FileSystem::from_spi(MemorySpi {
            state: Arc::clone(&state),
            provider_id,
        })
        .expect("memory SPI properties must be valid");
        Self {
            file_system,
            state,
            path_calls,
        }
    }

    /// Builds one absolute logical path for the fixture namespace.
    pub(super) fn path_for(relative: &str) -> FixtureResult<Path> {
        Path::parse(&format!("/contract/{relative}")).map_err(|error| FixtureError::new(error.to_string()))
    }

    /// Returns whether the fixture namespace contains no resources.
    pub fn is_empty(&self) -> bool {
        self.entry_count() == 0
    }

    /// Returns the number of resources retained by the fixture namespace.
    pub fn entry_count(&self) -> usize {
        self.state.lock().expect("memory state lock must succeed").entries.len()
    }

    /// Changes the injected provider fault for subsequent operations.
    pub fn set_fault(&self, fault: MemoryFault) {
        self.state.lock().expect("memory state lock must succeed").fault = fault;
    }

    /// Returns how many facade deletion attempts the provider has received.
    pub fn delete_attempt_count(&self) -> usize {
        self.state
            .lock()
            .expect("memory state lock must succeed")
            .delete_attempts
    }

    /// Returns how many contract paths the suite requested from this fixture.
    pub fn path_call_count(&self) -> usize {
        self.path_calls.load(Ordering::Relaxed)
    }

    fn publish(&self, path: &Path, entry: Entry) {
        let mut state = self.state.lock().expect("memory state lock must succeed");
        publish_entry(&mut state, path.as_str(), entry);
    }
}

impl FileSystemFixture for MemoryFixture {
    /// Reads the entire isolated model without calling the facade.
    fn snapshot_namespace_paths(&self) -> FixtureResult<FixtureSupport<Vec<Path>>> {
        let state = self.state.lock().expect("memory state lock");
        let paths = state
            .entries
            .keys()
            .map(|key| Path::parse(key).map_err(|error| FixtureError::with_source("invalid stored path", error)))
            .collect::<FixtureResult<Vec<_>>>()?;
        Ok(FixtureSupport::Supported(paths))
    }

    fn prepare_read(
        &self,
        scenario: testkit::ReadScenario,
        relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<testkit::FixturePreparation<Path>> {
        let unavailable = self.state.lock().expect("memory state lock").unavailable_case;
        let excluded = match scenario {
            testkit::ReadScenario::IfMatchCurrent | testkit::ReadScenario::IfMatchStale => {
                unavailable == Some(UnavailableScenario::ReadIfMatch)
            }
            testkit::ReadScenario::IfNoneMatchCurrent | testkit::ReadScenario::IfNoneMatchStale => {
                unavailable == Some(UnavailableScenario::ReadIfNoneMatch)
            }
            _ => false,
        };
        if excluded {
            return Ok(testkit::FixturePreparation::Unavailable {
                reason: "selected read scenario preparation is unavailable".to_owned(),
            });
        }
        let prepared = if scenario == testkit::ReadScenario::ChecksumCorruption {
            self.checksum_failure_case(relative)?
        } else {
            self.seed_file(relative, bytes)?
        };
        Ok(match prepared {
            FixtureSupport::Supported(path) => testkit::FixturePreparation::Ready(path),
            FixtureSupport::Unsupported => testkit::FixturePreparation::Unavailable {
                reason: "independent read setup is unavailable".to_owned(),
            },
        })
    }

    fn file_system(&self) -> &FileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.path_calls.fetch_add(1, Ordering::Relaxed);
        Self::path_for(relative)
    }

    /// Prepares fresh requests independently, including explicit absence
    /// conditions.
    fn prepare_copy(
        &self,
        scenario: testkit::CopyScenario,
        source_relative: &str,
        target_relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<testkit::FixturePreparation<testkit::CopyFixtureCase>> {
        if scenario == testkit::CopyScenario::Conflict
            && self.state.lock().expect("memory state lock").unavailable_case
                == Some(UnavailableScenario::CopyOverwrite)
        {
            return Ok(testkit::FixturePreparation::Unavailable {
                reason: "copy conflict setup unavailable".to_owned(),
            });
        }
        if scenario == testkit::CopyScenario::ServerSide {
            if self.state.lock().expect("memory state lock").unavailable_case
                == Some(UnavailableScenario::Capability(FileSystemCapability::ServerSideCopy))
            {
                return Ok(testkit::FixturePreparation::Unavailable {
                    reason: "server-side copy setup unavailable".to_owned(),
                });
            }
            return Ok(match self.copy_fast_path_case(qfs::copy::CopyMethod::ServerSide)? {
                FixtureSupport::Supported(case) => testkit::FixturePreparation::Ready(case),
                FixtureSupport::Unsupported => testkit::FixturePreparation::Unavailable {
                    reason: "fixture has no server-side copy case".to_owned(),
                },
            });
        }
        let required = match scenario {
            testkit::CopyScenario::AtomicFile => Some(FileSystemCapability::AtomicFileCopy),
            testkit::CopyScenario::AtomicTree => Some(FileSystemCapability::AtomicTreeCopy),
            testkit::CopyScenario::DurableTree => Some(FileSystemCapability::DurableTreeCopy),
            testkit::CopyScenario::DurableFile => Some(FileSystemCapability::DurableFileCopy),
            _ => None,
        };
        if required.is_some_and(|capability| {
            self.state.lock().expect("memory state lock").unavailable_case
                == Some(UnavailableScenario::Capability(capability))
        }) {
            return Ok(testkit::FixturePreparation::Unavailable {
                reason: "strong file-copy setup unavailable".to_owned(),
            });
        }
        if matches!(
            scenario,
            testkit::CopyScenario::AtomicTree | testkit::CopyScenario::DurableTree
        ) {
            if self.state.lock().expect("memory state lock").unavailable_case == Some(UnavailableScenario::CopyTree) {
                return Ok(testkit::FixturePreparation::Unavailable {
                    reason: "tree copy setup unavailable".to_owned(),
                });
            }
            let FixtureSupport::Supported(source) = self.seed_empty_directory(source_relative)? else {
                return Ok(testkit::FixturePreparation::Unavailable {
                    reason: "tree root setup unavailable".to_owned(),
                });
            };
            let sub_relative = format!("{source_relative}/sub");
            if matches!(self.seed_empty_directory(&sub_relative)?, FixtureSupport::Unsupported) {
                return Ok(testkit::FixturePreparation::Unavailable {
                    reason: "tree subdirectory setup unavailable".to_owned(),
                });
            }
            let child_relative = format!("{source_relative}/sub/child");
            if matches!(self.seed_file(&child_relative, bytes)?, FixtureSupport::Unsupported) {
                return Ok(testkit::FixturePreparation::Unavailable {
                    reason: "tree child setup unavailable".to_owned(),
                });
            }
            let target = self.path(target_relative)?;
            let options = if scenario == testkit::CopyScenario::AtomicTree {
                qfs::copy::CopyOptions::tree().with_atomicity(qfs::metadata::AtomicityRequirement::Required)
            } else {
                qfs::copy::CopyOptions::tree().with_durability(qfs::metadata::DurabilityRequirement::Required)
            };
            return Ok(testkit::FixturePreparation::Ready(testkit::CopyFixtureCase::new(
                source, target, options,
            )));
        }
        let FixtureSupport::Supported(source) = self.seed_file(source_relative, bytes)? else {
            return Ok(testkit::FixturePreparation::Unavailable {
                reason: "copy source seed unavailable".to_owned(),
            });
        };
        let target = if scenario == testkit::CopyScenario::Conflict {
            let FixtureSupport::Supported(target) = self.seed_file(target_relative, b"existing")? else {
                return Ok(testkit::FixturePreparation::Unavailable {
                    reason: "copy conflict target seed unavailable".to_owned(),
                });
            };
            target
        } else {
            self.path(target_relative)?
        };
        let options = match scenario {
            testkit::CopyScenario::AtomicFile => {
                qfs::copy::CopyOptions::file().with_atomicity(qfs::metadata::AtomicityRequirement::Required)
            }
            testkit::CopyScenario::DurableFile => {
                qfs::copy::CopyOptions::file().with_durability(qfs::metadata::DurabilityRequirement::Required)
            }
            _ => qfs::copy::CopyOptions::file(),
        };
        Ok(testkit::FixturePreparation::Ready(testkit::CopyFixtureCase::new(
            source, target, options,
        )))
    }

    fn prepare_delete(
        &self,
        scenario: testkit::DeleteScenario,
        relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<testkit::FixturePreparation<Path>> {
        if scenario == testkit::DeleteScenario::IfMatch
            && self.state.lock().expect("memory state lock").unavailable_case
                == Some(UnavailableScenario::DeleteIfMatch)
        {
            return Ok(testkit::FixturePreparation::Unavailable {
                reason: "conditional delete setup unavailable".to_owned(),
            });
        }
        Ok(match self.seed_file(relative, bytes)? {
            FixtureSupport::Supported(path) => testkit::FixturePreparation::Ready(path),
            FixtureSupport::Unsupported => testkit::FixturePreparation::Unavailable {
                reason: "delete seed unavailable".to_owned(),
            },
        })
    }

    fn prepare_write(
        &self,
        scenario: testkit::WriteScenario,
        relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<testkit::FixturePreparation<testkit::WriteFixtureCase>> {
        let unavailable = self
            .state
            .lock()
            .expect("memory state lock must succeed")
            .unavailable_case;
        if scenario == testkit::WriteScenario::IfAbsent && unavailable == Some(UnavailableScenario::WriteIfAbsent) {
            return Ok(testkit::FixturePreparation::Unavailable {
                reason: "fixture cannot prepare If-Absent case".to_owned(),
            });
        }
        if scenario == testkit::WriteScenario::IfMatch {
            if unavailable == Some(UnavailableScenario::WriteIfMatch) {
                return Ok(testkit::FixturePreparation::Unavailable {
                    reason: "fixture cannot prepare If-Match case".to_owned(),
                });
            }
            let FixtureSupport::Supported(path) = self.seed_file(relative, b"a")? else {
                return Ok(testkit::FixturePreparation::Unavailable {
                    reason: "conditional seed unavailable".to_owned(),
                });
            };
            let FixtureSupport::Supported(version) = self.resource_version(&path)? else {
                return Ok(testkit::FixturePreparation::Unavailable {
                    reason: "conditional version unavailable".to_owned(),
                });
            };
            return Ok(testkit::FixturePreparation::Ready(testkit::WriteFixtureCase::new(
                path,
                bytes.to_vec(),
                WriteOptions::default().with_precondition(WritePrecondition::IfMatch(version)),
            )));
        }
        if scenario == testkit::WriteScenario::Replace {
            let FixtureSupport::Supported(path) = self.seed_file(relative, b"previous contents")? else {
                return Ok(testkit::FixturePreparation::Unavailable {
                    reason: "replacement seed unavailable".to_owned(),
                });
            };
            return Ok(testkit::FixturePreparation::Ready(testkit::WriteFixtureCase::new(
                path,
                bytes.to_vec(),
                qfs::write::WriteOptions::default(),
            )));
        }
        if scenario == testkit::WriteScenario::CreateConflict {
            let FixtureSupport::Supported(path) = self.seed_file(relative, b"a")? else {
                return Ok(testkit::FixturePreparation::Unavailable {
                    reason: "creation conflict seed unavailable".to_owned(),
                });
            };
            return Ok(testkit::FixturePreparation::Ready(testkit::WriteFixtureCase::new(
                path,
                bytes.to_vec(),
                qfs::write::WriteOptions::default().with_disposition(qfs::write::WriteDisposition::CreateNew),
            )));
        }
        if scenario == testkit::WriteScenario::Append {
            let FixtureSupport::Supported(path) = self.seed_file(relative, b"before")? else {
                return Ok(testkit::FixturePreparation::Unavailable {
                    reason: "append seed unavailable".to_owned(),
                });
            };
            return Ok(testkit::FixturePreparation::Ready(testkit::WriteFixtureCase::new(
                path,
                bytes.to_vec(),
                qfs::write::WriteOptions::default().with_disposition(qfs::write::WriteDisposition::Append),
            )));
        }
        if scenario == testkit::WriteScenario::AtomicReplace {
            let FixtureSupport::Supported(path) = self.seed_file(relative, b"a")? else {
                return Ok(testkit::FixturePreparation::Unavailable {
                    reason: "atomic replacement seed unavailable".to_owned(),
                });
            };
            return Ok(testkit::FixturePreparation::Ready(testkit::WriteFixtureCase::new(
                path,
                bytes.to_vec(),
                qfs::write::WriteOptions::default().with_atomicity(qfs::metadata::AtomicityRequirement::Required),
            )));
        }
        if scenario == testkit::WriteScenario::Durable {
            return Ok(testkit::FixturePreparation::Ready(testkit::WriteFixtureCase::new(
                self.path(relative)?,
                bytes.to_vec(),
                qfs::write::WriteOptions::default()
                    .with_disposition(qfs::write::WriteDisposition::CreateNew)
                    .with_durability(qfs::metadata::DurabilityRequirement::Required),
            )));
        }
        let options = match scenario {
            testkit::WriteScenario::Create | testkit::WriteScenario::Abort => {
                WriteOptions::default().with_disposition(WriteDisposition::CreateNew)
            }
            testkit::WriteScenario::IfAbsent => WriteOptions::default().with_precondition(WritePrecondition::IfAbsent),
            _ => {
                return Ok(testkit::FixturePreparation::Unavailable {
                    reason: "write scenario preparation unavailable".to_owned(),
                });
            }
        };
        Ok(testkit::FixturePreparation::Ready(testkit::WriteFixtureCase::new(
            self.path(relative)?,
            bytes.to_vec(),
            options,
        )))
    }

    fn copy_fallback_only(&self) -> bool {
        self.state.lock().expect("memory state lock must succeed").fallback_only
    }

    fn seed_file(&self, relative: &str, bytes: &[u8]) -> FixtureResult<FixtureSupport<Path>> {
        let path = Self::path_for(relative)?;
        let mut state = self.state.lock().expect("memory state lock must succeed");
        publish_entry(&mut state, path.as_str(), Entry::File(bytes.to_vec()));
        Ok(FixtureSupport::Supported(path))
    }

    fn read_file(&self, path: &Path) -> FixtureResult<FixtureSupport<Vec<u8>>> {
        let state = self.state.lock().expect("memory state lock must succeed");
        let entry = state.entries.get(path.as_str()).cloned();
        Ok(match entry {
            Some(Entry::File(bytes)) => FixtureSupport::Supported(bytes),
            Some(Entry::Directory | Entry::Symlink) | None => FixtureSupport::Unsupported,
        })
    }

    fn resource_version(&self, path: &Path) -> FixtureResult<FixtureSupport<ResourceVersion>> {
        let state = self.state.lock().expect("memory state lock must succeed");
        Ok(if state.entries.contains_key(path.as_str()) {
            FixtureSupport::Supported(ResourceVersion::new(format!(
                "v{}",
                state.versions.get(path.as_str()).copied().unwrap_or(1)
            )))
        } else {
            FixtureSupport::Unsupported
        })
    }

    fn stale_resource_version(&self, path: &Path) -> FixtureResult<FixtureSupport<ResourceVersion>> {
        let state = self.state.lock().expect("memory state lock must succeed");
        let current = state.versions.get(path.as_str()).copied().unwrap_or(1);
        Ok(FixtureSupport::Supported(ResourceVersion::new(format!(
            "v{}",
            current.saturating_sub(1)
        ))))
    }

    fn exists_out_of_band(&self, path: &Path) -> FixtureResult<FixtureSupport<bool>> {
        Ok(FixtureSupport::Supported(
            self.state
                .lock()
                .expect("memory state lock must succeed")
                .entries
                .contains_key(path.as_str()),
        ))
    }

    fn write_file_out_of_band(&self, path: &Path, bytes: &[u8]) -> FixtureResult<FixtureSupport<()>> {
        self.publish(path, Entry::File(bytes.to_vec()));
        Ok(FixtureSupport::Supported(()))
    }

    fn checksum_failure_case(&self, relative: &str) -> FixtureResult<FixtureSupport<Path>> {
        if self.state.lock().expect("memory state lock must succeed").fault == MemoryFault::ChecksumIgnoresCorruption {
            return Ok(FixtureSupport::Unsupported);
        }
        let path = Self::path_for(relative)?;
        self.publish(&path, Entry::File(b"checksum-source".to_vec()));
        Ok(FixtureSupport::Supported(path))
    }

    fn teardown(&self) -> FixtureResult<()> {
        let mut state = self.state.lock().expect("memory state lock must succeed");
        // These self-test faults also inject an out-of-band teardown failure
        // so retry tests retain real resources. Never claim successful
        // teardown while deliberately retaining fixture-owned state.
        if !matches!(
            state.fault,
            MemoryFault::DeleteNoOp
                | MemoryFault::CleanupStatError
                | MemoryFault::CleanupDeleteError
                | MemoryFault::CleanupDeletePanic
        ) {
            state.entries.clear();
            state.versions.clear();
        } else {
            return Err(FixtureError::new("injected independent teardown failure"));
        }
        Ok(())
    }

    fn seed_empty_directory(&self, relative: &str) -> FixtureResult<FixtureSupport<Path>> {
        let path = Self::path_for(relative)?;
        self.state
            .lock()
            .expect("memory state lock must succeed")
            .entries
            .insert(path.as_str().to_owned(), Entry::Directory);
        let mut state = self.state.lock().expect("memory state lock must succeed");
        publish_entry(&mut state, path.as_str(), Entry::Directory);
        Ok(FixtureSupport::Supported(path))
    }

    fn seed_symlink(&self, relative: &str) -> FixtureResult<FixtureSupport<Path>> {
        let path = Self::path_for(relative)?;
        self.state
            .lock()
            .expect("memory state lock must succeed")
            .entries
            .insert(path.as_str().to_owned(), Entry::Symlink);
        let mut state = self.state.lock().expect("memory state lock must succeed");
        publish_entry(&mut state, path.as_str(), Entry::Symlink);
        Ok(FixtureSupport::Supported(path))
    }

    fn copy_fast_path_case(&self, method: CopyMethod) -> FixtureResult<FixtureSupport<CopyFixtureCase>> {
        if method != CopyMethod::ServerSide {
            return Ok(FixtureSupport::Unsupported);
        }
        let source = Self::path_for("server-side-copy-source")?;
        let target = Self::path_for("server-side-copy-target")?;
        self.state
            .lock()
            .expect("memory state lock must succeed")
            .entries
            .insert(source.as_str().to_owned(), Entry::File(b"server-side".to_vec()));
        Ok(FixtureSupport::Supported(CopyFixtureCase::new(
            source,
            target,
            CopyOptions::file().with_server_side(ServerSidePreference::Require),
        )))
    }
}

struct MemorySpi {
    state: Arc<Mutex<State>>,
    provider_id: &'static str,
}

impl MemorySpi {
    /// Returns a safe provider error for an unsupported primitive.
    fn unsupported(operation: FsOperation) -> FsError {
        FsError::new(
            FsErrorKind::UnsupportedOperation,
            operation,
            "unused memory SPI operation",
        )
    }

    /// Returns the provider identity for an opened temporary resource.
    fn info(path: Path) -> OpenedFileInfo {
        OpenedFileInfo::new(
            FileSystemId::new("memory-contract").expect("memory provider id must be valid"),
            path,
        )
    }

    /// Allocates one temporary resource path and inserts its entry.
    fn create_temp(&self, directory: bool, parent: Option<&Path>, prefix: &str, suffix: &str) -> Path {
        let mut state = self.state.lock().expect("memory state lock must succeed");
        let parent = if state.fault == MemoryFault::TempIgnoresOptions {
            "/contract"
        } else {
            parent.map_or("/contract", Path::as_str)
        };
        let prefix = if state.fault == MemoryFault::TempIgnoresOptions {
            if directory { ".tmp-dir-" } else { ".tmp-" }
        } else {
            prefix
        };
        let suffix = if state.fault == MemoryFault::TempIgnoresOptions {
            ""
        } else {
            suffix
        };
        let path = Path::parse(&format!("{parent}/{prefix}{}{suffix}", state.next_temp))
            .expect("generated temporary path must be valid");
        state.next_temp += 1;
        publish_entry(
            &mut state,
            path.as_str(),
            if directory {
                Entry::Directory
            } else {
                Entry::File(Vec::new())
            },
        );
        path
    }
}

impl FileSystemSpi for MemorySpi {
    fn properties(&self) -> ProviderProperties {
        let state = self.state.lock().expect("memory state lock must succeed");
        let mut capabilities = FileSystemCapabilities::new();
        if state.optional_capabilities {
            capabilities = capabilities
                .with_guaranteed(FileSystemCapability::Rename)
                .with_guaranteed(FileSystemCapability::TempFile)
                .with_guaranteed(FileSystemCapability::TempDirectory)
                .with_guaranteed(FileSystemCapability::Append)
                .with_guaranteed(FileSystemCapability::AtomicRename)
                .with_guaranteed(FileSystemCapability::AtomicReplace)
                .with_guaranteed(FileSystemCapability::AtomicFileCopy)
                .with_guaranteed(FileSystemCapability::DurableFileCopy)
                .with_guaranteed(FileSystemCapability::DurableRename)
                .with_guaranteed(FileSystemCapability::DurableWrite)
                .with_guaranteed(FileSystemCapability::AtomicTempPersist)
                .with_guaranteed(FileSystemCapability::ServerSideCopy);
        }
        if state.create_directory_capability {
            capabilities = capabilities.with_guaranteed(FileSystemCapability::CreateDirectory);
        }
        if state.extended_capabilities {
            for capability in [
                FileSystemCapability::RangeRead,
                FileSystemCapability::ConditionalRead,
                FileSystemCapability::ChecksumValidation,
                FileSystemCapability::ConditionalWrite,
                FileSystemCapability::EmptyDirectory,
                FileSystemCapability::ConditionalDelete,
                FileSystemCapability::Symlink,
                FileSystemCapability::AtomicTreeCopy,
                FileSystemCapability::DurableTreeCopy,
            ] {
                capabilities = capabilities.with_guaranteed(capability);
            }
        }
        if state.core_capabilities {
            capabilities = capabilities
                .with_guaranteed(FileSystemCapability::Read)
                .with_guaranteed(FileSystemCapability::List);
            capabilities = capabilities.with_guaranteed(FileSystemCapability::Copy);
            if !state.read_only {
                capabilities = capabilities.with_guaranteed(FileSystemCapability::Write);
            }
        }
        if state.delete_capability {
            capabilities = capabilities.with_guaranteed(FileSystemCapability::Delete);
            if state.optional_capabilities {
                capabilities = capabilities.with_guaranteed(FileSystemCapability::RecursiveDelete);
            }
        }
        let limits = state.limits;
        drop(state);
        provider_properties_with_copy(
            FileSystemProperties::new(
                FileSystemInfo::new(
                    FileSystemId::new("memory-contract").expect("memory provider id must be valid"),
                    self.provider_id,
                    PathSemantics::Hierarchical,
                ),
                capabilities,
                limits,
                PathConstraints::absolute(),
                SymlinkPolicy::Reject,
            )
            .expect("memory properties must be valid"),
            true,
        )
    }

    fn stat(&self, request: StatRequest<'_>) -> FsResult<StatResponse> {
        let state = self.state.lock().expect("memory state lock must succeed");
        if state.fault == MemoryFault::CleanupStatError {
            return Err(FsError::new(
                FsErrorKind::PermissionDenied,
                FsOperation::Stat,
                "cleanup stat error",
            ));
        }
        let Some(entry) = state.entries.get(request.path().as_str()) else {
            return Err(FsError::new(
                FsErrorKind::NotFound,
                FsOperation::Stat,
                "memory entry absent",
            ));
        };
        let kind = match (state.fault, entry) {
            (MemoryFault::WrongStatKind, Entry::File(_)) => FileKind::Directory,
            (MemoryFault::ObjectKinds, Entry::File(_)) => FileKind::Object,
            (MemoryFault::ObjectKinds, Entry::Directory) => FileKind::Prefix,
            (_, Entry::File(_)) => FileKind::File,
            (_, Entry::Directory) => FileKind::Directory,
            (_, Entry::Symlink) => FileKind::Symlink,
        };
        let mut metadata = FileMetadata::new(kind);
        if let Entry::File(bytes) = entry {
            metadata = metadata.with_len(Some(bytes.len() as u64));
        }
        Ok(StatResponse::new(request.path().clone(), metadata))
    }

    fn list(&self, request: ListRequest<'_>) -> FsResult<OpenedDirectoryStream> {
        let state = self.state.lock().expect("memory state lock must succeed");
        let entries = if state.fault == MemoryFault::EmptyList {
            Vec::new()
        } else {
            listed_entries(
                &state.entries,
                request.scope().path().expect("hierarchical fixture scope"),
                request.options().options(),
                state.fault != MemoryFault::ListDropsMetadata,
            )
        };
        Ok(OpenedDirectoryStream::new(Box::new(MemoryDirectoryStream {
            entries: entries.into_iter(),
        })))
    }

    fn open_reader(&self, request: OpenReaderRequest<'_>) -> FsResult<OpenedReader> {
        let state = self.state.lock().expect("memory state lock must succeed");
        let Some(Entry::File(bytes)) = state.entries.get(request.path().as_str()) else {
            return Err(FsError::new(
                FsErrorKind::NotFound,
                FsOperation::OpenReader,
                "memory entry absent",
            ));
        };
        if request.options().options().if_match().as_ref().is_some_and(|version| {
            version.as_str() != format!("v{}", state.versions.get(request.path().as_str()).copied().unwrap_or(1))
                && state.fault != MemoryFault::IgnoreReadIfMatch
        }) || request
            .options()
            .options()
            .if_none_match()
            .as_ref()
            .is_some_and(|version| {
                version.as_str() == format!("v{}", state.versions.get(request.path().as_str()).copied().unwrap_or(1))
                    && state.fault != MemoryFault::IgnoreReadIfNoneMatch
            })
        {
            return Err(FsError::new(
                FsErrorKind::PreconditionFailed,
                FsOperation::OpenReader,
                "memory read condition failed",
            ));
        }
        let options = request.options().options();
        if options.checksum() == ChecksumPolicy::Required && request.path().as_str().contains("checksum-failure") {
            return Err(FsError::new(
                FsErrorKind::DataCorruption,
                FsOperation::OpenReader,
                "memory checksum probe detected corruption",
            ));
        }
        let mut bytes = if state.fault == MemoryFault::ReadWrongBytes
            || (state.fault == MemoryFault::ChecksumIgnoresCorruption && options.checksum() == ChecksumPolicy::Required)
        {
            b"wrong bytes".to_vec()
        } else {
            bytes.clone()
        };
        let start = options.offset().unwrap_or(0).min(bytes.len() as u64) as usize;
        let end = options.length().map_or(bytes.len(), |length| {
            start.saturating_add(length as usize).min(bytes.len())
        });
        bytes = bytes[start..end].to_vec();
        Ok(OpenedReader::new(
            Self::info(request.path().clone()),
            Box::new(Cursor::new(bytes)),
        ))
    }

    fn open_writer(&self, request: OpenWriterRequest<'_>) -> FsResult<OpenedWriter> {
        Ok(OpenedWriter::new(
            Self::info(request.path().clone()),
            Box::new(MemoryWriter {
                state: Arc::clone(&self.state),
                path: request.path().clone(),
                bytes: Vec::new(),
                disposition: request.options().options().disposition(),
                atomicity: request.options().options().atomicity(),
                durability: request.options().options().durability(),
                precondition: request.options().options().precondition().clone(),
            }),
        ))
    }

    fn create_directory(&self, request: CreateDirectoryRequest<'_>) -> FsResult<CreateDirectoryOutcome> {
        let mut state = self.state.lock().expect("memory state lock must succeed");
        let already_existed = state.entries.contains_key(request.path().as_str());
        if request.options().options().recursive() && state.fault != MemoryFault::RecursiveCreateLeavesParentsMissing {
            for (index, _) in request
                .path()
                .as_str()
                .match_indices('/')
                .filter(|(index, _)| *index > 0)
            {
                let ancestor = &request.path().as_str()[..index];
                if !state.entries.contains_key(ancestor) {
                    publish_entry(&mut state, ancestor, Entry::Directory);
                }
            }
        }
        if !already_existed {
            publish_entry(&mut state, request.path().as_str(), Entry::Directory);
        }
        Ok(CreateDirectoryOutcome::new(already_existed))
    }

    fn delete_file(&self, request: DeleteFileRequest<'_>) -> FsResult<DeleteOutcome> {
        let cleanup_panics =
            self.state.lock().expect("memory state lock must succeed").fault == MemoryFault::CleanupDeletePanic;
        if cleanup_panics {
            panic!("cleanup delete panic");
        }
        let mut state = self.state.lock().expect("memory state lock must succeed");
        if state.fault == MemoryFault::CleanupDeleteError {
            return Err(FsError::new(
                FsErrorKind::PermissionDenied,
                FsOperation::Delete,
                "cleanup delete error",
            ));
        }
        state.delete_attempts = state.delete_attempts.saturating_add(1);
        let existed = state.entries.contains_key(request.path().as_str());
        if let Some(version) = request.options().options().if_match()
            && existed
            && version.as_str() != format!("v{}", state.versions.get(request.path().as_str()).copied().unwrap_or(1))
            && state.fault != MemoryFault::IgnoreDeleteIfMatch
        {
            return Err(FsError::new(
                FsErrorKind::PreconditionFailed,
                FsOperation::Delete,
                "memory delete condition failed",
            ));
        }
        let removed = if state.fault == MemoryFault::DeleteNoOp {
            None
        } else {
            remove_entry(&mut state, request.path().as_str())
        };
        Ok(DeleteOutcome::new(removed.is_none() && !existed))
    }

    fn delete_directory(&self, request: DeleteDirectoryRequest<'_>) -> FsResult<DeleteOutcome> {
        let cleanup_panics =
            self.state.lock().expect("memory state lock must succeed").fault == MemoryFault::CleanupDeletePanic;
        if cleanup_panics {
            panic!("cleanup delete panic");
        }
        let mut state = self.state.lock().expect("memory state lock must succeed");
        if state.fault == MemoryFault::CleanupDeleteError {
            return Err(FsError::new(
                FsErrorKind::PermissionDenied,
                FsOperation::Delete,
                "cleanup delete error",
            ));
        }
        state.delete_attempts = state.delete_attempts.saturating_add(1);
        let existed = state.entries.contains_key(request.path().as_str());
        let already_missing = if state.fault == MemoryFault::DeleteNoOp {
            false
        } else {
            let removed = remove_entry(&mut state, request.path().as_str());
            let mut removed_descendant = false;
            if request.options().options().recursive() && state.fault != MemoryFault::RecursiveDeleteLeavesChildren {
                let prefix = format!("{}/", request.path().as_str().trim_end_matches('/'));
                let before = state.entries.len();
                let descendants = state
                    .entries
                    .keys()
                    .filter(|path| path.starts_with(&prefix))
                    .cloned()
                    .collect::<Vec<_>>();
                state.entries.retain(|path, _| !path.starts_with(&prefix));
                for path in descendants {
                    state.versions.remove(&path);
                }
                removed_descendant = state.entries.len() != before;
            }
            removed.is_none() && !removed_descendant && !existed
        };
        Ok(DeleteOutcome::new(already_missing))
    }

    fn try_copy(&self, request: CopyRequest<'_>) -> Result<CopyAttempt, SpiCopyFailure> {
        let mut state = self.state.lock().expect("memory state lock must succeed");
        let options = request.options().options();
        if !state.fallback_only
            && (state.native_copy
                || options.mode() == CopyMode::Tree
                || options.server_side() == ServerSidePreference::Require
                || options.durability() == DurabilityRequirement::Required
                || options.conflict() == CopyConflictPolicy::Overwrite)
        {
            let Some(entry) = state.entries.get(request.source().as_str()).cloned() else {
                return Err(SpiCopyFailure::new(
                    FsError::new(FsErrorKind::NotFound, FsOperation::Copy, "memory copy source absent"),
                    CopyFailureState::Unchanged,
                    CopyStats::default(),
                ));
            };
            if state.entries.contains_key(request.target().as_str()) {
                match options.conflict() {
                    CopyConflictPolicy::Fail => {
                        return Err(SpiCopyFailure::new(
                            FsError::new(
                                FsErrorKind::AlreadyExists,
                                FsOperation::Copy,
                                "memory copy target already exists",
                            ),
                            CopyFailureState::Unchanged,
                            CopyStats::default(),
                        ));
                    }
                    CopyConflictPolicy::Skip => {
                        return Ok(CopyAttempt::Completed(CopyOutcome::new(
                            CopyStats {
                                skipped: 1,
                                ..CopyStats::default()
                            },
                            CopyMethod::Native,
                            AchievedAtomicity::NonAtomic,
                        )));
                    }
                    CopyConflictPolicy::Overwrite => {}
                }
            }
            let mut stats = match &entry {
                Entry::File(bytes) => CopyStats {
                    files: 1,
                    bytes: bytes.len() as u64,
                    ..CopyStats::default()
                },
                Entry::Directory => CopyStats {
                    directories: 1,
                    ..CopyStats::default()
                },
                Entry::Symlink => CopyStats {
                    symlinks: 1,
                    ..CopyStats::default()
                },
            };
            let overwritten = state.entries.contains_key(request.target().as_str())
                && options.conflict() == CopyConflictPolicy::Overwrite;
            stats.overwritten = u64::from(overwritten);
            if !(overwritten && state.fault == MemoryFault::CopyOverwriteKeepsTarget) {
                publish_entry(&mut state, request.target().as_str(), entry);
            }
            if matches!(state.entries.get(request.source().as_str()), Some(Entry::Directory))
                && state.fault != MemoryFault::DirectoryCopyDropsChildren
            {
                let source_prefix = format!("{}/", request.source().as_str().trim_end_matches('/'));
                let target_prefix = format!("{}/", request.target().as_str().trim_end_matches('/'));
                let descendants = state
                    .entries
                    .iter()
                    .filter_map(|(path, entry)| {
                        path.strip_prefix(&source_prefix)
                            .map(|relative| (format!("{target_prefix}{relative}"), entry.clone()))
                    })
                    .collect::<Vec<_>>();
                for (path, entry) in descendants {
                    match &entry {
                        Entry::File(bytes) => {
                            stats.files += 1;
                            stats.bytes += bytes.len() as u64;
                        }
                        Entry::Directory => stats.directories += 1,
                        Entry::Symlink => stats.symlinks += 1,
                    }
                    publish_entry(&mut state, &path, entry);
                }
            }
            if options.mode() == CopyMode::Tree && state.fault == MemoryFault::TreeCopyWrongStats {
                stats.bytes = 0;
            }
            let method = if options.server_side() == ServerSidePreference::Require
                && state.fault != MemoryFault::ServerSideCopyFallsBack
            {
                CopyMethod::ServerSide
            } else {
                CopyMethod::Native
            };
            return Ok(CopyAttempt::Completed(
                CopyOutcome::new(
                    stats,
                    method,
                    if options.atomicity() == AtomicityRequirement::Required
                        && !(options.mode() == CopyMode::File && state.fault == MemoryFault::AtomicFileCopyNonAtomic)
                        && !(options.mode() == CopyMode::Tree && state.fault == MemoryFault::AtomicTreeCopyNonAtomic)
                    {
                        AchievedAtomicity::Atomic
                    } else {
                        AchievedAtomicity::NonAtomic
                    },
                )
                .with_durable(
                    options.durability() == DurabilityRequirement::Required
                        && state.fault != MemoryFault::DurableFileCopyNonDurable
                        && !(options.mode() == CopyMode::Tree && state.fault == MemoryFault::DurableTreeCopyNonDurable),
                ),
            ));
        }
        Ok(CopyAttempt::Declined(CopyDeclineReason::NotImplemented))
    }

    fn rename(&self, request: RenameRequest<'_>) -> Result<RenameOutcome, SpiRenameFailure> {
        let mut state = self.state.lock().expect("memory state lock must succeed");
        let _durability = request.options().options().durability();
        if !request.options().options().overwrite() && state.entries.contains_key(request.target().as_str()) {
            return Err(SpiRenameFailure::new(
                FsError::new(
                    FsErrorKind::AlreadyExists,
                    FsOperation::Rename,
                    "memory rename target already exists",
                ),
                RenameFailureState::Unchanged,
            ));
        }
        let Some(entry) = remove_entry(&mut state, request.source().as_str()) else {
            return Err(SpiRenameFailure::new(
                FsError::new(FsErrorKind::NotFound, FsOperation::Rename, "memory entry absent"),
                RenameFailureState::Unchanged,
            ));
        };
        if state.fault != MemoryFault::RenameNoOp {
            remove_entry(&mut state, request.source().as_str());
            state.entries.insert(request.target().as_str().to_owned(), entry);
        } else {
            state.entries.insert(request.source().as_str().to_owned(), entry);
        }
        Ok(RenameOutcome::new(
            request.source().clone(),
            request.target().clone(),
            if request.options().options().atomicity() == AtomicityRequirement::Required
                && state.fault != MemoryFault::AtomicRenameNonAtomic
            {
                AchievedAtomicity::Atomic
            } else {
                AchievedAtomicity::NonAtomic
            },
            PublicationMethod::Direct,
        )
        .with_durable(
            request.options().options().durability() == DurabilityRequirement::Required
                && state.fault != MemoryFault::DurableRenameNonDurable,
        ))
    }

    fn create_temp_file(&self, request: CreateTempFileRequest) -> FsResult<OpenedTempFile> {
        let path = self.create_temp(
            false,
            request.options().parent(),
            request.options().prefix(),
            request.options().suffix(),
        );
        Ok(OpenedTempFile::new(
            Self::info(path.clone()).with_metadata(FileMetadata::new(FileKind::File)),
            Box::new(TempSession {
                state: Arc::clone(&self.state),
                path,
            }),
        ))
    }

    fn create_temp_directory(&self, request: CreateTempDirectoryRequest) -> FsResult<OpenedTempDirectory> {
        let path = self.create_temp(
            true,
            request.options().parent(),
            request.options().prefix(),
            request.options().suffix(),
        );
        Ok(OpenedTempDirectory::new(
            Self::info(path.clone()).with_metadata(FileMetadata::new(FileKind::Directory)),
            Box::new(TempSession {
                state: Arc::clone(&self.state),
                path,
            }),
        ))
    }
}

struct MemoryDirectoryStream {
    entries: std::vec::IntoIter<DirEntry>,
}

impl DirectoryStreamSpi for MemoryDirectoryStream {
    fn next_entry(&mut self) -> FsResult<Option<DirEntry>> {
        Ok(self.entries.next())
    }
}

pub(crate) fn listed_entries(
    entries: &HashMap<String, Entry>,
    root: &Path,
    options: &ListOptions,
    include_metadata: bool,
) -> Vec<DirEntry> {
    let prefix = format!("{}/", root.as_str().trim_end_matches('/'));
    entries
        .iter()
        .filter_map(|(text, entry)| {
            let relative = text.strip_prefix(&prefix)?;
            if relative.is_empty() || (!options.recursive() && options.prefix().is_none() && relative.contains('/')) {
                return None;
            }
            if !options.prefix().is_none_or(|prefix| {
                relative == prefix
                    || relative
                        .strip_prefix(prefix)
                        .is_some_and(|remaining| remaining.starts_with('/'))
            }) {
                return None;
            }
            let kind = match entry {
                Entry::File(_) => FileKind::File,
                Entry::Directory => FileKind::Directory,
                Entry::Symlink => FileKind::Symlink,
            };
            let mut result = DirEntry::new(
                Path::parse(text).expect("stored memory path must remain valid"),
                kind.clone(),
            );
            if options.include_metadata() && include_metadata {
                let mut metadata = FileMetadata::new(kind);
                if let Entry::File(bytes) = entry {
                    metadata = metadata.with_len(Some(bytes.len() as u64));
                }
                result.metadata = Some(metadata);
            }
            Some(result)
        })
        .collect()
}

struct MemoryWriter {
    state: Arc<Mutex<State>>,
    path: Path,
    bytes: Vec<u8>,
    disposition: WriteDisposition,
    atomicity: AtomicityRequirement,
    durability: DurabilityRequirement,
    precondition: WritePrecondition,
}

impl Output for MemoryWriter {
    type Item = u8;

    unsafe fn write_unchecked(&mut self, input: &[u8], index: usize, count: usize) -> IoResult<usize> {
        self.bytes.extend_from_slice(&input[index..index + count]);
        Ok(count)
    }

    fn flush(&mut self) -> IoResult<()> {
        Ok(())
    }
}

impl FileWriterSpi for MemoryWriter {
    fn commit(&mut self) -> Result<WriteOutcome, SpiWriteFailure> {
        let mut state = self.state.lock().expect("memory state lock must succeed");
        if state.fault == MemoryFault::WriteCommitFailure {
            return Err(SpiWriteFailure::new(
                FsError::new(
                    FsErrorKind::PermissionDenied,
                    FsOperation::CommitWriter,
                    "injected commit failure",
                ),
                WriteFailureState::NotPublished,
            ));
        }
        if self.disposition == WriteDisposition::CreateNew
            && state.entries.contains_key(self.path.as_str())
            && state.fault != MemoryFault::CreateNewOverwrites
        {
            return Err(SpiWriteFailure::new(
                FsError::new(
                    FsErrorKind::AlreadyExists,
                    FsOperation::CommitWriter,
                    "memory destination already exists",
                ),
                WriteFailureState::NotPublished,
            ));
        }
        if self.precondition == WritePrecondition::IfAbsent && state.entries.contains_key(self.path.as_str()) {
            return Err(SpiWriteFailure::new(
                FsError::new(
                    FsErrorKind::PreconditionFailed,
                    FsOperation::CommitWriter,
                    "memory destination violates if-absent",
                ),
                WriteFailureState::NotPublished,
            ));
        }
        if let WritePrecondition::IfMatch(expected) = &self.precondition {
            let actual = state
                .versions
                .get(self.path.as_str())
                .copied()
                .map_or_else(|| "v0".to_owned(), |version| format!("v{version}"));
            if expected.as_str() != actual && state.fault != MemoryFault::IgnoreWriteIfMatch {
                return Err(SpiWriteFailure::new(
                    FsError::new(
                        FsErrorKind::PreconditionFailed,
                        FsOperation::CommitWriter,
                        "memory destination version mismatch",
                    ),
                    WriteFailureState::NotPublished,
                ));
            }
        }
        if state.fault != MemoryFault::WriteDropsBytes {
            let bytes = if self.disposition == WriteDisposition::CreateOrReplace
                && state.fault == MemoryFault::ReplaceKeepsSuffix
            {
                let mut combined = self.bytes.clone();
                if let Some(Entry::File(existing)) = state.entries.get(self.path.as_str()) {
                    combined.extend_from_slice(existing.get(self.bytes.len()..).unwrap_or_default());
                }
                combined
            } else if self.disposition == WriteDisposition::Append && state.fault != MemoryFault::AppendOverwrites {
                match state.entries.get(self.path.as_str()) {
                    Some(Entry::File(existing)) => [existing.as_slice(), self.bytes.as_slice()].concat(),
                    Some(Entry::Directory | Entry::Symlink) | None => self.bytes.clone(),
                }
            } else {
                self.bytes.clone()
            };
            if !(self.atomicity == AtomicityRequirement::Required
                && state.fault == MemoryFault::AtomicReplaceKeepsOldBytes
                || self.durability == DurabilityRequirement::Required
                    && state.fault == MemoryFault::DurableWriteDropsBytes)
            {
                publish_entry(&mut state, self.path.as_str(), Entry::File(bytes));
            }
        }
        Ok(WriteOutcome::new(
            if self.atomicity == AtomicityRequirement::Required && state.fault != MemoryFault::AtomicReplaceNonAtomic {
                AchievedAtomicity::Atomic
            } else {
                AchievedAtomicity::NonAtomic
            },
            PublicationMethod::Direct,
        )
        .with_durable(
            self.durability == DurabilityRequirement::Required && state.fault != MemoryFault::DurableWriteDropsBytes,
        ))
    }

    fn abort(&mut self) -> FsResult<WriteAbortOutcome> {
        if self.path.as_str().contains("write-aborted") {
            let mut state = self.state.lock().expect("memory state lock");
            if state.fault == MemoryFault::AbortFailsOnce {
                state.fault = MemoryFault::None;
                return Err(FsError::new(
                    FsErrorKind::PermissionDenied,
                    FsOperation::AbortWriter,
                    "abort failed once",
                ));
            }
            if state.fault == MemoryFault::AbortLies {
                state
                    .entries
                    .insert(self.path.as_str().to_owned(), Entry::File(self.bytes.clone()));
            }
        }
        Ok(WriteAbortOutcome::NotPublished)
    }
}

struct TempSession {
    state: Arc<Mutex<State>>,
    path: Path,
}

impl TempResourceSpi for TempSession {
    fn persist(&mut self, request: PersistRequest<'_>) -> Result<PersistOutcome, SpiPersistFailure> {
        let mut state = self.state.lock().expect("memory state lock must succeed");
        let entry = remove_entry(&mut state, self.path.as_str()).expect("temporary entry must exist");
        publish_entry(&mut state, request.target().as_str(), entry);
        let target = if state.fault == MemoryFault::WrongPersistTarget {
            Path::parse("/contract/wrong-persist-target").expect("generated path must be valid")
        } else {
            request.target().clone()
        };
        Ok(PersistOutcome::new(
            target,
            if request.options().atomicity() == AtomicityRequirement::Required
                && state.fault != MemoryFault::AtomicTempPersistNonAtomic
            {
                AchievedAtomicity::Atomic
            } else {
                AchievedAtomicity::NonAtomic
            },
            PublicationMethod::Direct,
        ))
    }

    fn keep(&mut self) -> Result<PersistOutcome, SpiPersistFailure> {
        let target = keep_target(&self.path);
        let mut state = self.state.lock().expect("memory state lock must succeed");
        let entry = remove_entry(&mut state, self.path.as_str()).expect("temporary entry must exist");
        publish_entry(&mut state, target.as_str(), entry);
        Ok(PersistOutcome::new(
            target,
            AchievedAtomicity::Atomic,
            PublicationMethod::Direct,
        ))
    }

    fn cleanup(&mut self) -> FsResult<()> {
        let mut state = self.state.lock().expect("memory state lock must succeed");
        if state.fault != MemoryFault::KeepTempOnCleanup {
            remove_entry(&mut state, self.path.as_str());
        }
        Ok(())
    }
}
