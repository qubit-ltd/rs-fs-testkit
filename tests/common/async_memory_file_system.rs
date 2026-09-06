// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0 (the "License");
//    you may not use this file except in compliance with the License.
//    You may obtain a copy of the License at
//
//        http://www.apache.org/licenses/LICENSE-2.0
//
//    Unless required by applicable law or agreed to in writing, software
//    distributed under the License is distributed on an "AS IS" BASIS,
//    WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//    See the License for the specific language governing permissions and
//    limitations under the License.
// =============================================================================
//! A deliberately small asynchronous SPI-backed provider used to self-test
//! contract suites.

#![allow(dead_code)]

use std::collections::HashMap;
#[cfg(feature = "async")]
use std::future;
#[cfg(feature = "async")]
use std::future::Future;
use std::io::Result as IoResult;
#[cfg(feature = "async")]
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
#[cfg(feature = "async")]
use std::task::Context;
#[cfg(feature = "async")]
use std::task::Poll;
#[cfg(feature = "async")]
use std::task::Wake;
#[cfg(feature = "async")]
use std::task::Waker;

#[cfg(feature = "async")]
use qubit_fs::AsyncFileSystem;
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
#[cfg(feature = "async")]
use qubit_fs::spi::AsyncDirectoryStreamSession;
#[cfg(feature = "async")]
use qubit_fs::spi::AsyncFileSystemSpi;
#[cfg(feature = "async")]
use qubit_fs::spi::AsyncFileWriteSession;
#[cfg(feature = "async")]
use qubit_fs::spi::AsyncTempResourceSpi;
use qubit_fs::spi::CopyAttempt;
use qubit_fs::spi::CopyDeclineReason;
use qubit_fs::spi::CopyRequest;
use qubit_fs::spi::CreateDirectoryRequest;
use qubit_fs::spi::CreateTempDirectoryRequest;
use qubit_fs::spi::CreateTempFileRequest;
use qubit_fs::spi::DeleteDirectoryRequest;
use qubit_fs::spi::DeleteFileRequest;
use qubit_fs::spi::ListRequest;
use qubit_fs::spi::OpenReaderRequest;
use qubit_fs::spi::OpenWriterRequest;
#[cfg(feature = "async")]
use qubit_fs::spi::OpenedAsyncDirectoryStream;
#[cfg(feature = "async")]
use qubit_fs::spi::OpenedAsyncReader;
#[cfg(feature = "async")]
use qubit_fs::spi::OpenedAsyncTempDirectory;
#[cfg(feature = "async")]
use qubit_fs::spi::OpenedAsyncTempFile;
#[cfg(feature = "async")]
use qubit_fs::spi::OpenedAsyncWriter;
use qubit_fs::spi::PersistRequest;
use qubit_fs::spi::ProviderProperties;
use qubit_fs::spi::RenameRequest;
use qubit_fs::spi::SpiCopyFailure;
#[cfg(feature = "async")]
use qubit_fs::spi::SpiFuture;
use qubit_fs::spi::SpiPersistFailure;
use qubit_fs::spi::SpiRenameFailure;
use qubit_fs::spi::StatRequest;
use qubit_fs::spi::StatResponse;
use qubit_fs::temp::PersistOutcome;
use qubit_fs::write::WriteAbortOutcome;
use qubit_fs::write::WriteDisposition;
#[cfg(feature = "async")]
use qubit_fs::write::WriteFailure;
use qubit_fs::write::WriteFailureState;
use qubit_fs::write::WritePrecondition;
#[cfg(feature = "async")]
use qubit_fs_testkit::AsyncCopyCancellationStage;
#[cfg(feature = "async")]
use qubit_fs_testkit::AsyncCopyFixtureCase;
#[cfg(feature = "async")]
use qubit_fs_testkit::AsyncFileSystemFixture;
#[cfg(feature = "async")]
use qubit_fs_testkit::CopyCancellationProbe;
use qubit_fs_testkit::CopyFixtureCase;
use qubit_fs_testkit::FixtureCase;
#[cfg(feature = "async")]
use qubit_fs_testkit::FixtureError;
#[cfg(feature = "async")]
use qubit_fs_testkit::FixtureFuture;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;
#[cfg(feature = "async")]
use qubit_io::AsyncInput;
#[cfg(feature = "async")]
use qubit_io::AsyncOutput;

use super::MemoryFixture;
use super::memory_file_system::listed_entries;
use super::memory_file_system::provider_properties;
use super::shared_model::Entry;

#[cfg(feature = "async")]
struct WakeFlag(AtomicUsize);

#[cfg(feature = "async")]
impl Wake for WakeFlag {
    fn wake(self: Arc<Self>) {
        self.0.store(1, Ordering::Release);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.store(1, Ordering::Release);
    }
}

/// Drives a runtime-neutral future using its actual wake notifications.
#[cfg(feature = "async")]
pub(crate) fn run_controlled<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    let flag = Arc::new(WakeFlag(AtomicUsize::new(1)));
    let waker = Waker::from(Arc::clone(&flag));
    let mut context = Context::from_waker(&waker);
    for _ in 0..1024 {
        flag.0.store(0, Ordering::Release);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => assert!(
                flag.0.load(Ordering::Acquire) != 0,
                "controlled future returned Pending without scheduling a wake"
            ),
        }
    }
    panic!("controlled future exceeded the poll budget");
}

/// Async fixture whose copy pipeline exposes one real pending point per stage.
#[cfg(feature = "async")]
pub struct AsyncMemoryFixture {
    file_system: AsyncFileSystem,
    stage: Arc<Mutex<AsyncCopyCancellationStage>>,
    copy_gate: Arc<Mutex<CopyGate>>,
    entries: Arc<Mutex<HashMap<String, Entry>>>,
    versions: Arc<Mutex<HashMap<String, u64>>>,
    supports_cancellation_cases: bool,
    path_calls: Arc<AtomicUsize>,
    limits: FileSystemLimits,
    unavailable_case: Option<FixtureCase>,
}

/// Shared stage gate used by the asynchronous cancellation self-test.
#[cfg(feature = "async")]
struct CopyGate {
    stage: AsyncCopyCancellationStage,
    pending_before_reached: u8,
    target_reached: bool,
    armed: bool,
    waker: Option<Waker>,
}

#[cfg(feature = "async")]
impl CopyGate {
    /// Creates a disarmed gate with no retained caller waker.
    fn new() -> Self {
        Self {
            stage: AsyncCopyCancellationStage::NativeAttempt,
            pending_before_reached: 0,
            target_reached: false,
            armed: false,
            waker: None,
        }
    }

    /// Arms a stage with two deliberate pre-acknowledgement suspensions.
    fn arm(&mut self, stage: AsyncCopyCancellationStage) {
        self.stage = stage;
        self.pending_before_reached = 2;
        self.target_reached = false;
        self.armed = true;
        self.waker = None;
    }

    /// Returns whether this gate currently controls the supplied stage.
    fn controls(&self, stage: AsyncCopyCancellationStage) -> bool {
        self.armed && self.stage == stage
    }

    /// Suspends the provider operation until the target stage is reached.
    fn poll_stage(&mut self, stage: AsyncCopyCancellationStage, context: &Context<'_>) -> Poll<()> {
        if !self.armed || self.stage != stage {
            return Poll::Ready(());
        }
        self.waker = Some(context.waker().clone());
        if self.pending_before_reached != 0 {
            self.pending_before_reached -= 1;
            context.waker().wake_by_ref();
            return Poll::Pending;
        }
        if !self.target_reached {
            self.target_reached = true;
            context.waker().wake_by_ref();
        }
        Poll::Pending
    }

    /// Polls stage acknowledgement using the caller's real waker.
    fn poll_reached(&mut self, context: &Context<'_>) -> Poll<FixtureResult<()>> {
        if self.target_reached {
            return Poll::Ready(Ok(()));
        }
        if !self.armed {
            return Poll::Ready(Err(FixtureError::new(
                "asynchronous copy cancellation gate was disarmed before acknowledgement",
            )));
        }
        self.waker = Some(context.waker().clone());
        Poll::Pending
    }

    /// Releases the gate and wakes the operation owner, without doing I/O.
    fn disarm(&mut self) -> FixtureResult<()> {
        self.armed = false;
        self.target_reached = false;
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
        Ok(())
    }
}

/// Provider-owned cancellation probe backed by the memory fixture gate.
#[cfg(feature = "async")]
struct AsyncMemoryCopyCancellationProbe {
    case: AsyncCopyFixtureCase,
    gate: Arc<Mutex<CopyGate>>,
}

#[cfg(feature = "async")]
impl CopyCancellationProbe for AsyncMemoryCopyCancellationProbe {
    /// Returns the isolated request controlled by this probe.
    fn case(&self) -> &AsyncCopyFixtureCase {
        &self.case
    }

    /// Reports only the provider-observed target stage.
    fn poll_reached(&self, context: &mut Context<'_>) -> Poll<FixtureResult<()>> {
        self.gate
            .lock()
            .expect("async copy gate lock must succeed")
            .poll_reached(context)
    }

    /// Releases the gate without starting an asynchronous operation.
    fn disarm(&self) -> FixtureResult<()> {
        self.gate.lock().expect("async copy gate lock must succeed").disarm()
    }
}

#[cfg(feature = "async")]
impl Drop for AsyncMemoryCopyCancellationProbe {
    /// Ensures a dropped probe cannot leave a provider gate armed.
    fn drop(&mut self) {
        let _ = self.disarm();
    }
}

/// Future that drives one provider stage gate with caller wake notifications.
#[cfg(feature = "async")]
struct CopyGateFuture {
    gate: Arc<Mutex<CopyGate>>,
    stage: AsyncCopyCancellationStage,
}

#[cfg(feature = "async")]
impl Future for CopyGateFuture {
    type Output = ();

    /// Polls the gate and never completes while cancellation owns it.
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        self.gate
            .lock()
            .expect("async copy gate lock must succeed")
            .poll_stage(self.stage, context)
    }
}

/// Waits at a provider stage and remains pending after explicit disarm.
#[cfg(feature = "async")]
async fn wait_copy_gate(gate: Arc<Mutex<CopyGate>>, stage: AsyncCopyCancellationStage) -> ! {
    CopyGateFuture { gate, stage }.await;
    future::pending().await
}

/// A single injected asynchronous provider defect used by the self-test matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(feature = "async")]
pub enum AsyncMemoryFault {
    /// Behaves conformingly.
    None,
    /// Reports a missing path as an existing file.
    MissingPathExists,
    /// Returns directory metadata for an existing file.
    WrongStatMetadata,
    /// Returns bytes different from the provider's seeded content.
    ReadWrongBytes,
    /// Accepts writes but does not publish their bytes.
    WriteDropsBytes,
    /// Produces a listing entry outside the requested namespace.
    ListEscapesNamespace,
    /// Returns no entries for a non-empty requested directory.
    EmptyList,
    /// Omits metadata explicitly requested by the caller.
    ListDropsMetadata,
    /// Reports deletion success without removing the resource.
    DeleteNoOp,
    /// Reports copy success without publishing the target bytes.
    CopyDropsTarget,
    /// Reports rename success without moving the resource.
    RenameNoOp,
    /// Reports a rename outcome with identities different from the request.
    RenameWrongOutcome,
    /// Copies a directory root without its descendants.
    DirectoryCopyDropsChildren,
    /// Reports temporary cleanup success without removing the resource.
    TempCleanupNoOp,
    /// Appends by replacing existing bytes.
    AppendOverwrites,
    /// Removes only the requested directory during recursive deletion.
    RecursiveDeleteLeavesChildren,
    /// Reports non-atomic completion for a required atomic rename.
    AtomicRenameNonAtomic,
    /// Reports non-atomic completion for a required atomic replacement.
    AtomicReplaceNonAtomic,
    /// Reports non-durable completion for a required durable copy.
    DurableFileCopyNonDurable,
    /// Reports non-durable completion for a required durable rename.
    DurableRenameNonDurable,
    /// Publishes the requested destination but reports a different target.
    TempPersistWrongTarget,
    /// Reports non-atomic completion for required temporary persistence.
    AtomicTempPersistNonAtomic,
    /// Ignores temporary-resource parent and affix options.
    TempIgnoresOptions,
    /// Uses object and prefix metadata kinds for stored resources.
    ObjectKinds,
    /// Ignores a stale or current If-Match read condition.
    IgnoreReadIfMatch,
    /// Ignores a stale or current If-None-Match read condition.
    IgnoreReadIfNoneMatch,
    /// Ignores an If-Match write condition.
    IgnoreWriteIfMatch,
    /// Reports atomic replacement but leaves the old destination bytes.
    AtomicReplaceKeepsOldBytes,
    /// Reports durable write success but drops the published bytes.
    DurableWriteDropsBytes,
    /// Returns corrupted bytes instead of rejecting the checksum probe.
    ChecksumIgnoresCorruption,
}

/// Capability switches used by asynchronous memory fixture profiles.
#[derive(Clone, Copy)]
#[cfg(feature = "async")]
struct AsyncCapabilityProfile {
    core: bool,
    optional: bool,
    create_directory: bool,
    extended: bool,
    read_only: bool,
}

#[cfg(feature = "async")]
impl AsyncCapabilityProfile {
    const NONE: Self = Self {
        core: false,
        optional: false,
        create_directory: false,
        extended: false,
        read_only: false,
    };
    const CORE: Self = Self {
        core: true,
        optional: false,
        create_directory: false,
        extended: false,
        read_only: false,
    };
    const STANDARD: Self = Self {
        core: true,
        optional: true,
        create_directory: true,
        extended: false,
        read_only: false,
    };
    const PREFIX_DELETE: Self = Self {
        core: true,
        optional: true,
        create_directory: false,
        extended: false,
        read_only: false,
    };
    const ALL: Self = Self {
        core: true,
        optional: true,
        create_directory: true,
        extended: true,
        read_only: false,
    };
}

#[cfg(feature = "async")]
impl AsyncMemoryFixture {
    /// Creates an isolated asynchronous copy fixture.
    pub fn new() -> Self {
        Self::with_fault(AsyncMemoryFault::None)
    }

    /// Creates a fixture exposing only asynchronous read operations.
    pub fn read_only() -> Self {
        Self::with_configuration_options(
            AsyncMemoryFault::None,
            false,
            false,
            AsyncCapabilityProfile {
                read_only: true,
                ..AsyncCapabilityProfile::NONE
            },
            "async-memory-read-only-provider",
            FileSystemLimits::unknown(),
            None,
        )
    }

    /// Creates a fixture with a provider-declared limits snapshot.
    pub fn with_limits(limits: FileSystemLimits) -> Self {
        Self::with_configuration_options(
            AsyncMemoryFault::None,
            false,
            false,
            AsyncCapabilityProfile::STANDARD,
            "async-memory-limits-provider",
            limits,
            None,
        )
    }

    /// Creates a fixture whose selected conditional case is unavailable.
    pub fn with_conditional_case_unavailable(case: FixtureCase) -> Self {
        let mut fixture = Self::with_configuration(
            AsyncMemoryFault::None,
            false,
            false,
            AsyncCapabilityProfile::ALL,
            "async-memory-contract-provider",
        );
        fixture.unavailable_case = Some(case);
        fixture
    }

    /// Creates an asynchronous fixture with exactly one provider fault.
    pub fn with_fault(fault: AsyncMemoryFault) -> Self {
        Self::with_copy_behavior(fault, true, false)
    }

    /// Creates a conforming fixture without optional cancellation probes.
    pub fn without_cancellation_cases() -> Self {
        Self::with_copy_behavior(AsyncMemoryFault::None, false, false)
    }

    /// Creates a fixture whose provider completes copy through its native path.
    pub fn with_native_copy() -> Self {
        Self::with_copy_behavior(AsyncMemoryFault::None, false, true)
    }

    /// Creates a fixture without the core read, write, list, and copy
    /// capabilities.
    pub fn without_core_capabilities() -> Self {
        Self::with_configuration(
            AsyncMemoryFault::None,
            false,
            false,
            AsyncCapabilityProfile::NONE,
            "async-memory-contract-provider",
        )
    }

    /// Creates a fixture that advertises none of the suite operation
    /// capabilities.
    pub fn without_operation_capabilities() -> Self {
        Self::with_configuration(
            AsyncMemoryFault::None,
            false,
            false,
            AsyncCapabilityProfile::NONE,
            "async-memory-contract-provider",
        )
    }

    /// Creates an asynchronous fixture that exposes only core capabilities.
    pub fn without_optional_capabilities() -> Self {
        Self::with_configuration(
            AsyncMemoryFault::None,
            false,
            false,
            AsyncCapabilityProfile::CORE,
            "async-memory-contract-provider",
        )
    }

    /// Creates a conforming fixture whose filesystem and provider identifiers
    /// are identical.
    pub fn with_matching_ids() -> Self {
        Self::with_configuration(
            AsyncMemoryFault::None,
            false,
            false,
            AsyncCapabilityProfile::STANDARD,
            "async-memory-contract",
        )
    }

    /// Creates an asynchronous fixture using object and prefix metadata kinds.
    pub fn with_object_kinds() -> Self {
        Self::with_configuration(
            AsyncMemoryFault::ObjectKinds,
            false,
            false,
            AsyncCapabilityProfile::STANDARD,
            "async-memory-object-provider",
        )
    }

    /// Creates an asynchronous fixture supporting recursive prefix deletion
    /// without directory creation.
    pub fn recursive_delete_without_create_directory() -> Self {
        Self::with_configuration(
            AsyncMemoryFault::None,
            false,
            false,
            AsyncCapabilityProfile::PREFIX_DELETE,
            "async-memory-prefix-provider",
        )
    }

    /// Creates an asynchronous fixture advertising every capability contract.
    pub fn with_all_capabilities() -> Self {
        Self::with_configuration(
            AsyncMemoryFault::None,
            false,
            false,
            AsyncCapabilityProfile::ALL,
            "async-memory-all-capabilities-provider",
        )
    }

    /// Creates a fixture with selected copy and fault behavior.
    fn with_copy_behavior(fault: AsyncMemoryFault, supports_cancellation_cases: bool, native_copy: bool) -> Self {
        Self::with_configuration(
            fault,
            supports_cancellation_cases,
            native_copy,
            AsyncCapabilityProfile::ALL,
            "async-memory-contract-provider",
        )
    }

    /// Creates a fixture with selected capabilities and copy behavior.
    fn with_configuration(
        fault: AsyncMemoryFault,
        supports_cancellation_cases: bool,
        native_copy: bool,
        capabilities: AsyncCapabilityProfile,
        provider_id: &'static str,
    ) -> Self {
        Self::with_configuration_options(
            fault,
            supports_cancellation_cases,
            native_copy,
            capabilities,
            provider_id,
            FileSystemLimits::unknown(),
            None,
        )
    }

    /// Creates a fixture with all provider configuration knobs explicit.
    fn with_configuration_options(
        fault: AsyncMemoryFault,
        supports_cancellation_cases: bool,
        native_copy: bool,
        capabilities: AsyncCapabilityProfile,
        provider_id: &'static str,
        limits: FileSystemLimits,
        unavailable_case: Option<FixtureCase>,
    ) -> Self {
        let stage = Arc::new(Mutex::new(AsyncCopyCancellationStage::NativeAttempt));
        let copy_gate = Arc::new(Mutex::new(CopyGate::new()));
        let entries = Arc::new(Mutex::new(HashMap::new()));
        let versions = Arc::new(Mutex::new(HashMap::new()));
        let path_calls = Arc::new(AtomicUsize::new(0));
        let file_system = AsyncFileSystem::from_spi(AsyncMemorySpi {
            stage: Arc::clone(&stage),
            copy_gate: Arc::clone(&copy_gate),
            entries: Arc::clone(&entries),
            versions: Arc::clone(&versions),
            fault,
            native_copy,
            core_capabilities: capabilities.core,
            optional_capabilities: capabilities.optional,
            create_directory_capability: capabilities.create_directory,
            extended_capabilities: capabilities.extended,
            provider_id,
            limits,
            read_only: capabilities.read_only,
        })
        .expect("async memory SPI properties must be valid");
        Self {
            file_system,
            stage,
            copy_gate,
            entries,
            versions,
            supports_cancellation_cases,
            path_calls,
            limits,
            unavailable_case,
        }
    }

    /// Returns whether the fixture namespace contains no resources.
    pub fn is_empty(&self) -> bool {
        self.entries
            .lock()
            .expect("async memory state lock must succeed")
            .is_empty()
    }

    /// Returns how many contract paths the suite requested from this fixture.
    pub fn path_call_count(&self) -> usize {
        self.path_calls.load(Ordering::Relaxed)
    }
}

#[cfg(feature = "async")]
impl AsyncFileSystemFixture for AsyncMemoryFixture {
    fn file_system(&self) -> &AsyncFileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.path_calls.fetch_add(1, Ordering::Relaxed);
        MemoryFixture::path_for(relative)
    }

    fn case_support(&self, case: FixtureCase) -> FixtureResult<FixtureSupport<()>> {
        if self.unavailable_case == Some(case) {
            return Ok(FixtureSupport::Unsupported);
        }
        Ok(FixtureSupport::Supported(()))
    }

    fn seed_file<'a>(&'a self, relative: &'a str, bytes: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<Path>> {
        Box::pin(async move {
            let path = self.path(relative)?;
            self.entries
                .lock()
                .expect("async memory state lock must succeed")
                .insert(path.as_str().to_owned(), Entry::File(bytes.to_vec()));
            bump_version(&self.versions, &path);
            Ok(FixtureSupport::Supported(path))
        })
    }

    fn read_file<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<Vec<u8>>> {
        Box::pin(async move {
            let entry = self
                .entries
                .lock()
                .expect("async memory state lock must succeed")
                .get(path.as_str())
                .cloned();
            Ok(match entry {
                Some(Entry::File(bytes)) => FixtureSupport::Supported(bytes),
                Some(Entry::Directory | Entry::Symlink) | None => FixtureSupport::Unsupported,
            })
        })
    }

    fn exists_out_of_band<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<bool>> {
        Box::pin(async move {
            Ok(FixtureSupport::Supported(
                self.entries
                    .lock()
                    .expect("async memory state lock must succeed")
                    .contains_key(path.as_str()),
            ))
        })
    }

    fn write_file_out_of_band<'a>(&'a self, path: &'a Path, bytes: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<()>> {
        Box::pin(async move {
            self.entries
                .lock()
                .expect("async memory state lock must succeed")
                .insert(path.as_str().to_owned(), Entry::File(bytes.to_vec()));
            bump_version(&self.versions, path);
            Ok(FixtureSupport::Supported(()))
        })
    }

    fn resource_version<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<ResourceVersion>> {
        Box::pin(async move {
            let version = self
                .entries
                .lock()
                .expect("async memory state lock must succeed")
                .contains_key(path.as_str());
            Ok(if version {
                FixtureSupport::Supported(current_version(&self.versions, path))
            } else {
                FixtureSupport::Unsupported
            })
        })
    }

    fn stale_resource_version<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<ResourceVersion>> {
        Box::pin(async move {
            let exists = self
                .entries
                .lock()
                .expect("async memory state lock must succeed")
                .contains_key(path.as_str());
            Ok(if exists {
                FixtureSupport::Supported(stale_version(&self.versions, path))
            } else {
                FixtureSupport::Unsupported
            })
        })
    }

    fn checksum_failure_case<'a>(&'a self, relative: &'a str) -> FixtureFuture<'a, FixtureSupport<Path>> {
        Box::pin(async move {
            let path = self.path(relative)?;
            self.entries
                .lock()
                .expect("async memory state lock must succeed")
                .insert(path.as_str().to_owned(), Entry::File(b"checksum bytes".to_vec()));
            bump_version(&self.versions, &path);
            Ok(FixtureSupport::Supported(path))
        })
    }

    fn teardown(&self) -> FixtureFuture<'_, FixtureSupport<()>> {
        Box::pin(async move {
            self.entries
                .lock()
                .expect("async memory state lock must succeed")
                .clear();
            Ok(FixtureSupport::Supported(()))
        })
    }

    fn seed_empty_directory<'a>(&'a self, relative: &'a str) -> FixtureFuture<'a, FixtureSupport<Path>> {
        Box::pin(async move {
            let path = self.path(relative)?;
            self.entries
                .lock()
                .expect("async memory state lock must succeed")
                .insert(path.as_str().to_owned(), Entry::Directory);
            Ok(FixtureSupport::Supported(path))
        })
    }

    fn seed_symlink<'a>(&'a self, relative: &'a str) -> FixtureFuture<'a, FixtureSupport<Path>> {
        Box::pin(async move {
            let path = self.path(relative)?;
            self.entries
                .lock()
                .expect("async memory state lock must succeed")
                .insert(path.as_str().to_owned(), Entry::Symlink);
            Ok(FixtureSupport::Supported(path))
        })
    }

    fn copy_fast_path_case<'a>(&'a self, method: CopyMethod) -> FixtureFuture<'a, FixtureSupport<CopyFixtureCase>> {
        Box::pin(async move {
            if method != CopyMethod::ServerSide {
                return Ok(FixtureSupport::Unsupported);
            }
            let source = self.path("async-server-side-copy-source")?;
            let target = self.path("async-server-side-copy-target")?;
            self.entries
                .lock()
                .expect("async memory state lock must succeed")
                .insert(source.as_str().to_owned(), Entry::File(b"server-side".to_vec()));
            Ok(FixtureSupport::Supported(CopyFixtureCase::new(
                source,
                target,
                CopyOptions::default().with_server_side(ServerSidePreference::Require),
            )))
        })
    }

    fn prepare_copy_cancellation<'a>(
        &'a self,
        stage: AsyncCopyCancellationStage,
        relative: &'a str,
    ) -> FixtureFuture<'a, FixtureSupport<Box<dyn CopyCancellationProbe>>> {
        Box::pin(async move {
            if !self.supports_cancellation_cases {
                return Ok(FixtureSupport::Unsupported);
            }
            *self.stage.lock().expect("async stage lock must succeed") = stage;
            self.copy_gate
                .lock()
                .expect("async copy gate lock must succeed")
                .arm(stage);
            let source_relative = format!("{relative}-source");
            let target_relative = format!("{relative}-target");
            let source = self.path(&source_relative)?;
            let target = self.path(&target_relative)?;
            self.entries
                .lock()
                .expect("async memory state lock must succeed")
                .insert(source.as_str().to_owned(), Entry::File(b"copy bytes".to_vec()));
            let probe: Box<dyn CopyCancellationProbe> = Box::new(AsyncMemoryCopyCancellationProbe {
                case: AsyncCopyFixtureCase::new(source, target, CopyOptions::default()),
                gate: Arc::clone(&self.copy_gate),
            });
            Ok(FixtureSupport::Supported(probe))
        })
    }

    fn copy_cancellation_case(
        &self,
        stage: AsyncCopyCancellationStage,
    ) -> FixtureResult<FixtureSupport<AsyncCopyFixtureCase>> {
        if !self.supports_cancellation_cases {
            return Ok(FixtureSupport::Unsupported);
        }
        *self.stage.lock().expect("async stage lock must succeed") = stage;
        let source = self.path("async-copy-source")?;
        self.entries
            .lock()
            .expect("async memory state lock must succeed")
            .insert(source.as_str().to_owned(), Entry::File(b"copy bytes".to_vec()));
        Ok(FixtureSupport::Supported(AsyncCopyFixtureCase::new(
            source,
            self.path("async-copy-target")?,
            CopyOptions::default(),
        )))
    }
}

#[cfg(feature = "async")]
fn bump_version(versions: &Arc<Mutex<HashMap<String, u64>>>, path: &Path) {
    let mut versions = versions.lock().expect("async memory version lock must succeed");
    let version = versions.entry(path.as_str().to_owned()).or_insert(0);
    *version = version.saturating_add(1);
}

#[cfg(feature = "async")]
fn current_version(versions: &Arc<Mutex<HashMap<String, u64>>>, path: &Path) -> ResourceVersion {
    let version = versions
        .lock()
        .expect("async memory version lock must succeed")
        .get(path.as_str())
        .copied()
        .unwrap_or(1);
    ResourceVersion::new(format!("v{version}"))
}

#[cfg(feature = "async")]
fn stale_version(versions: &Arc<Mutex<HashMap<String, u64>>>, path: &Path) -> ResourceVersion {
    let version = versions
        .lock()
        .expect("async memory version lock must succeed")
        .get(path.as_str())
        .copied()
        .unwrap_or(1);
    if version <= 1 {
        ResourceVersion::new("stale-v0")
    } else {
        ResourceVersion::new(format!("v{}", version - 1))
    }
}

#[cfg(feature = "async")]
struct AsyncMemorySpi {
    stage: Arc<Mutex<AsyncCopyCancellationStage>>,
    copy_gate: Arc<Mutex<CopyGate>>,
    entries: Arc<Mutex<HashMap<String, Entry>>>,
    versions: Arc<Mutex<HashMap<String, u64>>>,
    fault: AsyncMemoryFault,
    native_copy: bool,
    core_capabilities: bool,
    optional_capabilities: bool,
    create_directory_capability: bool,
    extended_capabilities: bool,
    provider_id: &'static str,
    limits: FileSystemLimits,
    read_only: bool,
}

#[cfg(feature = "async")]
impl AsyncMemorySpi {
    /// Reads the currently selected pending stage.
    fn stage(&self) -> AsyncCopyCancellationStage {
        *self.stage.lock().expect("async stage lock must succeed")
    }

    /// Returns whether a cancellation probe currently controls this stage.
    fn gate_controls(&self, stage: AsyncCopyCancellationStage) -> bool {
        self.copy_gate
            .lock()
            .expect("async copy gate lock must succeed")
            .controls(stage)
    }

    /// Returns the fixed provider identity for a opened handle.
    fn info(path: &Path) -> OpenedFileInfo {
        OpenedFileInfo::new(
            FileSystemId::new("async-memory-contract").expect("async provider id must be valid"),
            path.clone(),
        )
    }

    /// Returns the safe error used by unused methods.
    fn unused(operation: FsOperation) -> FsError {
        FsError::new(
            FsErrorKind::UnsupportedOperation,
            operation,
            "unused async memory SPI operation",
        )
    }
}

#[cfg(feature = "async")]
impl AsyncFileSystemSpi for AsyncMemorySpi {
    fn properties(&self) -> ProviderProperties {
        let mut capabilities = FileSystemCapabilities::new();
        if self.optional_capabilities {
            capabilities = capabilities
                .with_guaranteed(FileSystemCapability::Delete)
                .with_guaranteed(FileSystemCapability::Rename)
                .with_guaranteed(FileSystemCapability::TempFile)
                .with_guaranteed(FileSystemCapability::TempDirectory)
                .with_guaranteed(FileSystemCapability::Append)
                .with_guaranteed(FileSystemCapability::RecursiveDelete)
                .with_guaranteed(FileSystemCapability::AtomicRename)
                .with_guaranteed(FileSystemCapability::AtomicReplace)
                .with_guaranteed(FileSystemCapability::AtomicFileCopy)
                .with_guaranteed(FileSystemCapability::DurableFileCopy)
                .with_guaranteed(FileSystemCapability::DurableRename)
                .with_guaranteed(FileSystemCapability::DurableWrite)
                .with_guaranteed(FileSystemCapability::AtomicTempPersist)
                .with_guaranteed(FileSystemCapability::ServerSideCopy);
        }
        if self.create_directory_capability {
            capabilities = capabilities.with_guaranteed(FileSystemCapability::CreateDirectory);
        }
        if self.extended_capabilities {
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
        if self.core_capabilities {
            capabilities = capabilities
                .with_guaranteed(FileSystemCapability::Copy)
                .with_guaranteed(FileSystemCapability::Read)
                .with_guaranteed(FileSystemCapability::Write)
                .with_guaranteed(FileSystemCapability::List);
        }
        if self.read_only {
            capabilities = capabilities.with_guaranteed(FileSystemCapability::Read);
        }
        provider_properties(
            FileSystemProperties::new(
                FileSystemInfo::new(
                    FileSystemId::new("async-memory-contract").expect("async provider id must be valid"),
                    self.provider_id,
                    PathSemantics::Hierarchical,
                ),
                capabilities,
                self.limits,
                PathConstraints::absolute(),
                SymlinkPolicy::Reject,
            )
            .expect("async memory properties must be valid"),
        )
    }

    fn stat<'a>(&'a self, request: StatRequest<'a>) -> SpiFuture<'a, FsResult<StatResponse>> {
        let path = request.path().clone();
        let entry = self
            .entries
            .lock()
            .expect("async memory state lock must succeed")
            .get(path.as_str())
            .cloned();
        let fault = self.fault;
        Box::pin(async move {
            match entry {
                Some(Entry::File(bytes)) => {
                    let mut metadata = FileMetadata::new(if fault == AsyncMemoryFault::ObjectKinds {
                        FileKind::Object
                    } else {
                        FileKind::File
                    });
                    metadata = metadata.with_len(Some(bytes.len() as u64));
                    if fault == AsyncMemoryFault::WrongStatMetadata {
                        metadata = metadata.with_kind(FileKind::Directory).with_len(None);
                    }
                    Ok(StatResponse::new(path, metadata))
                }
                Some(Entry::Directory) => Ok(StatResponse::new(
                    path,
                    FileMetadata::new(if fault == AsyncMemoryFault::ObjectKinds {
                        FileKind::Prefix
                    } else {
                        FileKind::Directory
                    }),
                )),
                Some(Entry::Symlink) => Ok(StatResponse::new(path, FileMetadata::new(FileKind::Symlink))),
                None if fault == AsyncMemoryFault::MissingPathExists => {
                    Ok(StatResponse::new(path, FileMetadata::new(FileKind::File)))
                }
                None => Err(FsError::new(
                    FsErrorKind::NotFound,
                    FsOperation::Stat,
                    "async memory entry absent",
                )),
            }
        })
    }

    fn list<'a>(&'a self, request: ListRequest<'a>) -> SpiFuture<'a, FsResult<OpenedAsyncDirectoryStream>> {
        let entries = if self.fault == AsyncMemoryFault::ListEscapesNamespace {
            vec![DirEntry::new(
                Path::parse("/outside-list-root").expect("fixed list entry path must be valid"),
                FileKind::Directory,
            )]
        } else if self.fault == AsyncMemoryFault::EmptyList {
            Vec::new()
        } else {
            listed_entries(
                &self.entries.lock().expect("async memory state lock must succeed"),
                request.path(),
                request.options().options(),
                self.fault != AsyncMemoryFault::ListDropsMetadata,
            )
        };
        Box::pin(async move {
            Ok(OpenedAsyncDirectoryStream::new(Box::new(AsyncMemoryDirectoryStream {
                entries: entries.into_iter(),
            })))
        })
    }

    fn open_reader<'a>(&'a self, request: OpenReaderRequest<'a>) -> SpiFuture<'a, FsResult<OpenedAsyncReader>> {
        if self.gate_controls(AsyncCopyCancellationStage::Reader) {
            let gate = Arc::clone(&self.copy_gate);
            return Box::pin(async move { wait_copy_gate(gate, AsyncCopyCancellationStage::Reader).await });
        }
        let path = request.path().clone();
        let info = Self::info(&path);
        let bytes = self
            .entries
            .lock()
            .expect("async memory state lock must succeed")
            .get(path.as_str())
            .cloned();
        let fault = self.fault;
        let versions = Arc::clone(&self.versions);
        let options = request.options().options().clone();
        Box::pin(async move {
            let Some(Entry::File(mut bytes)) = bytes else {
                return Err(FsError::new(
                    FsErrorKind::NotFound,
                    FsOperation::OpenReader,
                    "async memory entry absent",
                ));
            };
            let current = current_version(&versions, &path);
            if (options
                .if_match()
                .as_ref()
                .is_some_and(|version| version.as_str() != current.as_str())
                && fault != AsyncMemoryFault::IgnoreReadIfMatch)
                || (options
                    .if_none_match()
                    .as_ref()
                    .is_some_and(|version| version.as_str() == current.as_str())
                    && fault != AsyncMemoryFault::IgnoreReadIfNoneMatch)
            {
                return Err(FsError::new(
                    FsErrorKind::PreconditionFailed,
                    FsOperation::OpenReader,
                    "async memory read condition failed",
                ));
            }
            if fault == AsyncMemoryFault::ReadWrongBytes
                || (fault == AsyncMemoryFault::ChecksumIgnoresCorruption
                    && options.checksum() == ChecksumPolicy::Required
                    && path.as_str().contains("checksum-failure"))
            {
                bytes = b"wrong bytes".to_vec();
            }
            if options.checksum() == ChecksumPolicy::Required
                && path.as_str().contains("checksum-failure")
                && fault != AsyncMemoryFault::ChecksumIgnoresCorruption
            {
                return Err(FsError::new(
                    FsErrorKind::DataCorruption,
                    FsOperation::Read,
                    "async memory checksum mismatch",
                ));
            }
            let start = options.offset().unwrap_or(0).min(bytes.len() as u64) as usize;
            let end = options.length().map_or(bytes.len(), |length| {
                start.saturating_add(length as usize).min(bytes.len())
            });
            bytes = bytes[start..end].to_vec();
            Ok(OpenedAsyncReader::new(
                info,
                Box::new(AsyncMemoryReader { bytes, offset: 0 }),
            ))
        })
    }

    fn open_writer<'a>(&'a self, request: OpenWriterRequest<'a>) -> SpiFuture<'a, FsResult<OpenedAsyncWriter>> {
        let selected_stage = self.stage();
        let stage = if self.gate_controls(AsyncCopyCancellationStage::Writer)
            || self.gate_controls(AsyncCopyCancellationStage::Commit)
        {
            selected_stage
        } else {
            AsyncCopyCancellationStage::NativeAttempt
        };
        let path = request.path().clone();
        let info = Self::info(&path);
        let state = Arc::clone(&self.entries);
        let versions = Arc::clone(&self.versions);
        let copy_gate = Arc::clone(&self.copy_gate);
        let fault = self.fault;
        let disposition = request.options().options().disposition();
        let atomicity = request.options().options().atomicity();
        let precondition = request.options().options().precondition().clone();
        let durability = request.options().options().durability();
        Box::pin(async move {
            Ok(OpenedAsyncWriter::new(
                info,
                Box::new(AsyncMemoryWriter {
                    stage,
                    state,
                    versions,
                    copy_gate,
                    path,
                    bytes: Vec::new(),
                    fault,
                    disposition,
                    atomicity,
                    durability,
                    precondition,
                }),
            ))
        })
    }

    fn create_directory<'a>(
        &'a self,
        request: CreateDirectoryRequest<'a>,
    ) -> SpiFuture<'a, FsResult<CreateDirectoryOutcome>> {
        let path = request.path().clone();
        let entries = Arc::clone(&self.entries);
        Box::pin(async move {
            let already_existed = entries
                .lock()
                .expect("async memory state lock must succeed")
                .insert(path.as_str().to_owned(), Entry::Directory)
                .is_some();
            Ok(CreateDirectoryOutcome::new(already_existed))
        })
    }

    fn delete_file<'a>(&'a self, request: DeleteFileRequest<'a>) -> SpiFuture<'a, FsResult<DeleteOutcome>> {
        let path = request.path().clone();
        let entries = Arc::clone(&self.entries);
        let fault = self.fault;
        Box::pin(async move {
            let missing = if fault == AsyncMemoryFault::DeleteNoOp {
                false
            } else {
                entries
                    .lock()
                    .expect("async memory state lock must succeed")
                    .remove(path.as_str())
                    .is_none()
            };
            Ok(DeleteOutcome::new(missing))
        })
    }

    fn delete_directory<'a>(&'a self, request: DeleteDirectoryRequest<'a>) -> SpiFuture<'a, FsResult<DeleteOutcome>> {
        let path = request.path().clone();
        let entries = Arc::clone(&self.entries);
        let recursive = request.options().options().recursive();
        let fault = self.fault;
        Box::pin(async move {
            let missing = if fault == AsyncMemoryFault::DeleteNoOp {
                true
            } else {
                let mut entries = entries.lock().expect("async memory state lock must succeed");
                let removed = entries.remove(path.as_str());
                let mut removed_descendant = false;
                if recursive && fault != AsyncMemoryFault::RecursiveDeleteLeavesChildren {
                    let prefix = format!("{}/", path.as_str().trim_end_matches('/'));
                    let before = entries.len();
                    entries.retain(|entry_path, _| !entry_path.starts_with(&prefix));
                    removed_descendant = entries.len() != before;
                }
                removed.is_none() && !removed_descendant
            };
            Ok(DeleteOutcome::new(missing))
        })
    }

    fn try_copy<'a>(&'a self, request: CopyRequest<'a>) -> SpiFuture<'a, Result<CopyAttempt, SpiCopyFailure>> {
        if self.gate_controls(AsyncCopyCancellationStage::NativeAttempt) {
            let gate = Arc::clone(&self.copy_gate);
            return Box::pin(async move { wait_copy_gate(gate, AsyncCopyCancellationStage::NativeAttempt).await });
        }
        let options = request.options().options();
        let mode = options.mode();
        let atomicity = options.atomicity();
        let durable = options.durability() == DurabilityRequirement::Required;
        let server_side = options.server_side() == ServerSidePreference::Require;
        let conflict = options.conflict();
        if self.native_copy
            || durable
            || server_side
            || options.mode() == CopyMode::Tree
            || conflict == CopyConflictPolicy::Overwrite
        {
            let source = request.source().clone();
            let target = request.target().clone();
            let entries = Arc::clone(&self.entries);
            let fault = self.fault;
            return Box::pin(async move {
                let mut entries = entries.lock().expect("async memory state lock must succeed");
                let Some(entry) = entries.get(source.as_str()).cloned() else {
                    return Err(SpiCopyFailure::new(
                        FsError::new(
                            FsErrorKind::NotFound,
                            FsOperation::Copy,
                            "async memory copy source absent",
                        ),
                        CopyFailureState::Unchanged,
                        CopyStats::default(),
                    ));
                };
                if entries.contains_key(target.as_str()) {
                    match conflict {
                        CopyConflictPolicy::Fail => {
                            return Err(SpiCopyFailure::new(
                                FsError::new(
                                    FsErrorKind::AlreadyExists,
                                    FsOperation::Copy,
                                    "async memory copy target already exists",
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
                let bytes = match &entry {
                    Entry::File(bytes) => bytes.len() as u64,
                    Entry::Directory | Entry::Symlink => 0,
                };
                let directory = matches!(&entry, Entry::Directory);
                let overwritten = entries.contains_key(target.as_str()) && conflict == CopyConflictPolicy::Overwrite;
                entries.insert(target.as_str().to_owned(), entry);
                if directory && fault != AsyncMemoryFault::DirectoryCopyDropsChildren {
                    let source_prefix = format!("{}/", source.as_str().trim_end_matches('/'));
                    let target_prefix = format!("{}/", target.as_str().trim_end_matches('/'));
                    let descendants = entries
                        .iter()
                        .filter_map(|(path, entry)| {
                            path.strip_prefix(&source_prefix)
                                .map(|relative| (format!("{target_prefix}{relative}"), entry.clone()))
                        })
                        .collect::<Vec<_>>();
                    for (path, entry) in descendants {
                        entries.insert(path, entry);
                    }
                }
                Ok(CopyAttempt::Completed(
                    CopyOutcome::new(
                        CopyStats {
                            files: 1,
                            bytes,
                            overwritten: u64::from(overwritten),
                            ..CopyStats::default()
                        },
                        if server_side {
                            CopyMethod::ServerSide
                        } else {
                            CopyMethod::Native
                        },
                        if atomicity == AtomicityRequirement::Required {
                            AchievedAtomicity::Atomic
                        } else {
                            AchievedAtomicity::NonAtomic
                        },
                    )
                    .with_durable(
                        durable && !(mode == CopyMode::File && fault == AsyncMemoryFault::DurableFileCopyNonDurable),
                    ),
                ))
            });
        }
        Box::pin(async { Ok(CopyAttempt::Declined(CopyDeclineReason::NotApplicable)) })
    }

    fn rename<'a>(&'a self, request: RenameRequest<'a>) -> SpiFuture<'a, Result<RenameOutcome, SpiRenameFailure>> {
        let source = request.source().clone();
        let target = request.target().clone();
        let entries = Arc::clone(&self.entries);
        let fault = self.fault;
        let atomicity = request.options().options().atomicity();
        let durability = request.options().options().durability();
        let overwrite = request.options().options().overwrite();
        Box::pin(async move {
            let mut entries = entries.lock().expect("async memory state lock must succeed");
            if !overwrite && entries.contains_key(target.as_str()) {
                return Err(SpiRenameFailure::new(
                    FsError::new(
                        FsErrorKind::AlreadyExists,
                        FsOperation::Rename,
                        "async memory rename target already exists",
                    ),
                    RenameFailureState::Unchanged,
                ));
            }
            if fault != AsyncMemoryFault::RenameNoOp {
                let Some(entry) = entries.remove(source.as_str()) else {
                    return Err(SpiRenameFailure::new(
                        FsError::new(FsErrorKind::NotFound, FsOperation::Rename, "async memory entry absent"),
                        RenameFailureState::Unchanged,
                    ));
                };
                entries.insert(target.as_str().to_owned(), entry);
            }
            drop(entries);
            let (reported_source, reported_target) = if fault == AsyncMemoryFault::RenameWrongOutcome {
                (
                    Path::parse("/contract/async-wrong-rename-source").expect("generated path must be valid"),
                    Path::parse("/contract/async-wrong-rename-target").expect("generated path must be valid"),
                )
            } else {
                (source, target)
            };
            Ok(RenameOutcome::new(
                reported_source,
                reported_target,
                if atomicity == AtomicityRequirement::Required && fault != AsyncMemoryFault::AtomicRenameNonAtomic {
                    AchievedAtomicity::Atomic
                } else {
                    AchievedAtomicity::NonAtomic
                },
                PublicationMethod::Direct,
            )
            .with_durable(
                durability == DurabilityRequirement::Required && fault != AsyncMemoryFault::DurableRenameNonDurable,
            ))
        })
    }

    fn create_temp_file<'a>(&'a self, request: CreateTempFileRequest) -> SpiFuture<'a, FsResult<OpenedAsyncTempFile>> {
        let entries = Arc::clone(&self.entries);
        let fault = self.fault;
        let options = request.options().clone();
        Box::pin(async move {
            let path = allocate_async_temp(
                &entries,
                false,
                options.parent(),
                options.prefix(),
                options.suffix(),
                fault,
            );
            Ok(OpenedAsyncTempFile::new(
                Self::info(&path).with_metadata(FileMetadata::new(FileKind::File)),
                Box::new(AsyncTempSession { entries, path, fault }),
            ))
        })
    }

    fn create_temp_directory<'a>(
        &'a self,
        request: CreateTempDirectoryRequest,
    ) -> SpiFuture<'a, FsResult<OpenedAsyncTempDirectory>> {
        let entries = Arc::clone(&self.entries);
        let fault = self.fault;
        let options = request.options().clone();
        Box::pin(async move {
            let path = allocate_async_temp(
                &entries,
                true,
                options.parent(),
                options.prefix(),
                options.suffix(),
                fault,
            );
            Ok(OpenedAsyncTempDirectory::new(
                Self::info(&path).with_metadata(FileMetadata::new(FileKind::Directory)),
                Box::new(AsyncTempSession { entries, path, fault }),
            ))
        })
    }
}

#[cfg(feature = "async")]
struct AsyncMemoryDirectoryStream {
    entries: std::vec::IntoIter<DirEntry>,
}

#[cfg(feature = "async")]
impl AsyncDirectoryStreamSession for AsyncMemoryDirectoryStream {
    fn next_entry_async<'a>(&'a mut self) -> SpiFuture<'a, FsResult<Option<DirEntry>>> {
        Box::pin(async move { Ok(self.entries.next()) })
    }
}

#[cfg(feature = "async")]
struct AsyncMemoryReader {
    bytes: Vec<u8>,
    offset: usize,
}

#[cfg(feature = "async")]
impl AsyncInput for AsyncMemoryReader {
    type Item = u8;

    unsafe fn poll_read_unchecked(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        output: &mut [u8],
        index: usize,
        count: usize,
    ) -> Poll<IoResult<usize>> {
        let this = self.get_mut();
        if count == 0 || this.offset == this.bytes.len() {
            Poll::Ready(Ok(0))
        } else {
            let length = count.min(this.bytes.len() - this.offset);
            output[index..index + length].copy_from_slice(&this.bytes[this.offset..this.offset + length]);
            this.offset += length;
            Poll::Ready(Ok(length))
        }
    }
}

#[cfg(feature = "async")]
struct AsyncMemoryWriter {
    stage: AsyncCopyCancellationStage,
    copy_gate: Arc<Mutex<CopyGate>>,
    state: Arc<Mutex<HashMap<String, Entry>>>,
    versions: Arc<Mutex<HashMap<String, u64>>>,
    path: Path,
    bytes: Vec<u8>,
    fault: AsyncMemoryFault,
    disposition: WriteDisposition,
    atomicity: AtomicityRequirement,
    durability: DurabilityRequirement,
    precondition: WritePrecondition,
}

#[cfg(feature = "async")]
impl AsyncOutput for AsyncMemoryWriter {
    type Item = u8;

    unsafe fn poll_write_unchecked(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        input: &[u8],
        index: usize,
        count: usize,
    ) -> Poll<IoResult<usize>> {
        let this = self.get_mut();
        if this.stage == AsyncCopyCancellationStage::Writer {
            match this
                .copy_gate
                .lock()
                .expect("async copy gate lock must succeed")
                .poll_stage(AsyncCopyCancellationStage::Writer, context)
            {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(()) => {}
            }
        }
        {
            this.bytes.extend_from_slice(&input[index..index + count]);
            Poll::Ready(Ok(count))
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<IoResult<()>> {
        let _ = self;
        Poll::Ready(Ok(()))
    }
}

#[cfg(feature = "async")]
impl AsyncFileWriteSession for AsyncMemoryWriter {
    fn commit_async<'a>(self: Pin<&'a mut Self>) -> SpiFuture<'a, Result<WriteOutcome, WriteFailure>> {
        if self.as_ref().get_ref().stage == AsyncCopyCancellationStage::Commit {
            let gate = Arc::clone(&self.as_ref().get_ref().copy_gate);
            return Box::pin(async move { wait_copy_gate(gate, AsyncCopyCancellationStage::Commit).await });
        }
        let this = self.get_mut();
        let state = Arc::clone(&this.state);
        let versions = Arc::clone(&this.versions);
        let path = this.path.clone();
        let bytes = this.bytes.clone();
        let fault = this.fault;
        let disposition = this.disposition;
        let atomicity = this.atomicity;
        let durability = this.durability;
        let precondition = this.precondition.clone();
        let bytes_written = bytes.len() as u64;
        Box::pin(async move {
            let destination_exists = state
                .lock()
                .expect("async memory state lock must succeed")
                .contains_key(path.as_str());
            if disposition == WriteDisposition::CreateNew && destination_exists {
                return Err(WriteFailure::new(
                    FsError::new(
                        FsErrorKind::AlreadyExists,
                        FsOperation::CommitWriter,
                        "async memory destination already exists",
                    ),
                    WriteFailureState::NotPublished,
                ));
            }
            if precondition == WritePrecondition::IfAbsent && destination_exists {
                return Err(WriteFailure::new(
                    FsError::new(
                        FsErrorKind::PreconditionFailed,
                        FsOperation::CommitWriter,
                        "async memory destination violates if-absent",
                    ),
                    WriteFailureState::NotPublished,
                ));
            }
            let current_version = current_version(&versions, &path);
            if let WritePrecondition::IfMatch(version) = &precondition
                && (version.as_str() != current_version.as_str() || !destination_exists)
                && fault != AsyncMemoryFault::IgnoreWriteIfMatch
            {
                return Err(WriteFailure::new(
                    FsError::new(
                        FsErrorKind::PreconditionFailed,
                        FsOperation::CommitWriter,
                        "async memory destination violates if-match",
                    ),
                    WriteFailureState::NotPublished,
                ));
            }
            if fault != AsyncMemoryFault::WriteDropsBytes
                && !(fault == AsyncMemoryFault::DurableWriteDropsBytes && durability == DurabilityRequirement::Required)
                && !(fault == AsyncMemoryFault::AtomicReplaceKeepsOldBytes
                    && atomicity == AtomicityRequirement::Required
                    && destination_exists)
                && !(fault == AsyncMemoryFault::CopyDropsTarget && path.as_str().contains("async-copy-positive-target"))
            {
                let mut state = state.lock().expect("async memory state lock must succeed");
                let bytes = if disposition == WriteDisposition::Append && fault != AsyncMemoryFault::AppendOverwrites {
                    let mut combined = match state.get(path.as_str()) {
                        Some(Entry::File(existing)) => existing.clone(),
                        Some(Entry::Directory | Entry::Symlink) | None => Vec::new(),
                    };
                    combined.extend_from_slice(&bytes);
                    combined
                } else {
                    bytes
                };
                state.insert(path.as_str().to_owned(), Entry::File(bytes));
                drop(state);
                bump_version(&versions, &path);
            }
            Ok(WriteOutcome::new(
                if atomicity == AtomicityRequirement::Required && fault != AsyncMemoryFault::AtomicReplaceNonAtomic {
                    AchievedAtomicity::Atomic
                } else {
                    AchievedAtomicity::NonAtomic
                },
                PublicationMethod::Direct,
            )
            .with_bytes_written(bytes_written)
            .with_durable(
                durability == DurabilityRequirement::Required && fault != AsyncMemoryFault::DurableWriteDropsBytes,
            ))
        })
    }

    fn abort_async<'a>(self: Pin<&'a mut Self>) -> SpiFuture<'a, FsResult<WriteAbortOutcome>> {
        let _ = self;
        Box::pin(async { Ok(WriteAbortOutcome::NotPublished) })
    }

    fn cancel_on_drop(self: Pin<&mut Self>) {
        let _ = self;
    }
}

/// Derives a fixture-local publication target from the temporary source path.
///
/// Keeping the source suffix makes independent temporary resources publish to
/// distinct paths without relying on a global mutable counter.
fn keep_target(source: &Path) -> Path {
    Path::parse(&format!("/kept{}", source.as_str())).expect("generated keep target must be valid")
}

/// Allocates an isolated path and inserts its temporary resource entry.
///
/// `entries` owns the fixture namespace and `directory` selects the resource
/// kind. The returned path is already present in that namespace.
#[cfg(feature = "async")]
fn allocate_async_temp(
    entries: &Arc<Mutex<HashMap<String, Entry>>>,
    directory: bool,
    parent: Option<&Path>,
    prefix: &str,
    suffix: &str,
    fault: AsyncMemoryFault,
) -> Path {
    let mut entries = entries.lock().expect("async memory state lock must succeed");
    let parent = if fault == AsyncMemoryFault::TempIgnoresOptions {
        "/contract"
    } else {
        parent.map_or("/contract", Path::as_str)
    };
    let (prefix, suffix) = if fault == AsyncMemoryFault::TempIgnoresOptions {
        (".async-tmp-", "")
    } else {
        (prefix, suffix)
    };
    let separator = if parent == "/" { "" } else { "/" };
    let path = Path::parse(&format!("{parent}{separator}{prefix}{}{suffix}", entries.len()))
        .expect("generated temporary path must be valid");
    entries.insert(
        path.as_str().to_owned(),
        if directory {
            Entry::Directory
        } else {
            Entry::File(Vec::new())
        },
    );
    path
}

/// Minimal asynchronous temporary-resource session for suite self-tests.
///
/// The session mutates the shared fixture namespace and applies its configured
/// fault when cleanup is requested.
#[cfg(feature = "async")]
struct AsyncTempSession {
    entries: Arc<Mutex<HashMap<String, Entry>>>,
    path: Path,
    fault: AsyncMemoryFault,
}

#[cfg(feature = "async")]
impl AsyncTempResourceSpi for AsyncTempSession {
    fn cleanup<'a>(self: Pin<&'a mut Self>) -> SpiFuture<'a, FsResult<()>> {
        let this = self.get_mut();
        let entries = Arc::clone(&this.entries);
        let path = this.path.clone();
        let fault = this.fault;
        Box::pin(async move {
            if fault != AsyncMemoryFault::TempCleanupNoOp {
                entries
                    .lock()
                    .expect("async memory state lock must succeed")
                    .remove(path.as_str());
            }
            Ok(())
        })
    }

    fn keep<'a>(self: Pin<&'a mut Self>) -> SpiFuture<'a, Result<PersistOutcome, SpiPersistFailure>> {
        let this = self.get_mut();
        let entries = Arc::clone(&this.entries);
        let source = this.path.clone();
        Box::pin(async move {
            let target = keep_target(&source);
            let mut entries = entries.lock().expect("async memory state lock must succeed");
            let entry = entries.remove(source.as_str()).expect("temporary entry must exist");
            entries.insert(target.as_str().to_owned(), entry);
            Ok(PersistOutcome::new(
                target,
                AchievedAtomicity::Atomic,
                PublicationMethod::Direct,
            ))
        })
    }

    fn persist<'a>(
        self: Pin<&'a mut Self>,
        request: PersistRequest<'a>,
    ) -> SpiFuture<'a, Result<PersistOutcome, SpiPersistFailure>> {
        let this = self.get_mut();
        let entries = Arc::clone(&this.entries);
        let source = this.path.clone();
        let target = request.target().clone();
        let atomicity = request.options().atomicity();
        let fault = this.fault;
        Box::pin(async move {
            let mut entries = entries.lock().expect("async memory state lock must succeed");
            let entry = entries.remove(source.as_str()).expect("temporary entry must exist");
            entries.insert(target.as_str().to_owned(), entry);
            let reported_target = if fault == AsyncMemoryFault::TempPersistWrongTarget {
                Path::parse("/contract/async-wrong-persist-target").expect("generated path must be valid")
            } else {
                target
            };
            Ok(PersistOutcome::new(
                reported_target,
                if atomicity == AtomicityRequirement::Required && fault != AsyncMemoryFault::AtomicTempPersistNonAtomic
                {
                    AchievedAtomicity::Atomic
                } else {
                    AchievedAtomicity::NonAtomic
                },
                PublicationMethod::Direct,
            ))
        })
    }
}
