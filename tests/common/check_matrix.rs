// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::panic::AssertUnwindSafe;

use super::memory_file_system::MemoryFault;
use crate::qubit_fs_testkit::FileSystemContract;

/// One synchronous provider fault and the contract check that must expose it.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct SyncFaultCase {
    /// The injected provider defect.
    pub fault: MemoryFault,
    /// The independently executable phase that owns the defect.
    pub phase: FileSystemContract,
    /// The stable check ID expected in the failure diagnostics.
    pub check_id: &'static str,
}

/// Returns the complete synchronous fault-to-check matrix.
#[allow(dead_code)]
pub const fn sync_fault_cases() -> &'static [SyncFaultCase] {
    &[
        SyncFaultCase {
            fault: MemoryFault::WrongStatKind,
            phase: FileSystemContract::Stat,
            check_id: "stat/file-kind",
        },
        SyncFaultCase {
            fault: MemoryFault::CleanupStatError,
            phase: FileSystemContract::CreateDirectory,
            check_id: "directory/create",
        },
        SyncFaultCase {
            fault: MemoryFault::KeepTempOnCleanup,
            phase: FileSystemContract::TempResources,
            check_id: "temp/file",
        },
        SyncFaultCase {
            fault: MemoryFault::WrongPersistTarget,
            phase: FileSystemContract::TempResources,
            check_id: "temp/file",
        },
        SyncFaultCase {
            fault: MemoryFault::EmptyList,
            phase: FileSystemContract::List,
            check_id: "list/basic",
        },
        SyncFaultCase {
            fault: MemoryFault::ReadWrongBytes,
            phase: FileSystemContract::Read,
            check_id: "read/basic",
        },
        SyncFaultCase {
            fault: MemoryFault::RecursiveCreateLeavesParentsMissing,
            phase: FileSystemContract::CreateDirectory,
            check_id: "directory/recursive",
        },
        SyncFaultCase {
            fault: MemoryFault::ReadFails,
            phase: FileSystemContract::Read,
            check_id: "read/basic",
        },
        SyncFaultCase {
            fault: MemoryFault::CreateDirectoryFails,
            phase: FileSystemContract::CreateDirectory,
            check_id: "directory/create",
        },
        SyncFaultCase {
            fault: MemoryFault::WriteDropsBytes,
            phase: FileSystemContract::Write,
            check_id: "write/basic",
        },
        SyncFaultCase {
            fault: MemoryFault::WriteFails,
            phase: FileSystemContract::Write,
            check_id: "write/basic",
        },
        SyncFaultCase {
            fault: MemoryFault::WriteCommitFailure,
            phase: FileSystemContract::Write,
            check_id: "write/basic",
        },
        SyncFaultCase {
            fault: MemoryFault::InvalidWriterIdentity,
            phase: FileSystemContract::Write,
            check_id: "write/basic",
        },
        SyncFaultCase {
            fault: MemoryFault::DeleteNoOp,
            phase: FileSystemContract::Delete,
            check_id: "delete/basic",
        },
        SyncFaultCase {
            fault: MemoryFault::RenameNoOp,
            phase: FileSystemContract::Rename,
            check_id: "rename/basic",
        },
        SyncFaultCase {
            fault: MemoryFault::ListDropsMetadata,
            phase: FileSystemContract::List,
            check_id: "list/prefix",
        },
        SyncFaultCase {
            fault: MemoryFault::DirectoryCopyDropsChildren,
            phase: FileSystemContract::Copy,
            check_id: "copy/atomic-tree",
        },
        SyncFaultCase {
            fault: MemoryFault::CopyOverwriteKeepsTarget,
            phase: FileSystemContract::Copy,
            check_id: "copy/fallback-overwrite-rejected",
        },
        SyncFaultCase {
            fault: MemoryFault::TempIgnoresOptions,
            phase: FileSystemContract::TempResources,
            check_id: "temp/file",
        },
        SyncFaultCase {
            fault: MemoryFault::TempCreationFails,
            phase: FileSystemContract::TempResources,
            check_id: "temp/file",
        },
        SyncFaultCase {
            fault: MemoryFault::TempKeepFails,
            phase: FileSystemContract::TempResources,
            check_id: "temp/file",
        },
        SyncFaultCase {
            fault: MemoryFault::TempPersistFails,
            phase: FileSystemContract::TempResources,
            check_id: "temp/atomic",
        },
        SyncFaultCase {
            fault: MemoryFault::TempCleanupFailsOnce,
            phase: FileSystemContract::TempResources,
            check_id: "temp/file",
        },
        SyncFaultCase {
            fault: MemoryFault::AppendOverwrites,
            phase: FileSystemContract::Write,
            check_id: "append/basic",
        },
        SyncFaultCase {
            fault: MemoryFault::CreateNewOverwrites,
            phase: FileSystemContract::Write,
            check_id: "write/create-conflict",
        },
        SyncFaultCase {
            fault: MemoryFault::ReplaceKeepsSuffix,
            phase: FileSystemContract::Write,
            check_id: "write/replace",
        },
        SyncFaultCase {
            fault: MemoryFault::AbortFailsOnce,
            phase: FileSystemContract::Write,
            check_id: "write/abort",
        },
        SyncFaultCase {
            fault: MemoryFault::ConditionalAbortFailsOnce,
            phase: FileSystemContract::Write,
            check_id: "write/create-conflict",
        },
        SyncFaultCase {
            fault: MemoryFault::AbortLies,
            phase: FileSystemContract::Write,
            check_id: "write/abort",
        },
        SyncFaultCase {
            fault: MemoryFault::ConditionalAbortReportsPublished,
            phase: FileSystemContract::Write,
            check_id: "write/if-absent",
        },
        SyncFaultCase {
            fault: MemoryFault::RecursiveDeleteLeavesChildren,
            phase: FileSystemContract::Delete,
            check_id: "delete/tree",
        },
        SyncFaultCase {
            fault: MemoryFault::AtomicRenameNonAtomic,
            phase: FileSystemContract::Rename,
            check_id: "rename/atomic",
        },
        SyncFaultCase {
            fault: MemoryFault::AtomicReplaceNonAtomic,
            phase: FileSystemContract::Write,
            check_id: "write/atomic-replace-existing",
        },
        SyncFaultCase {
            fault: MemoryFault::AtomicFileCopyNonAtomic,
            phase: FileSystemContract::Copy,
            check_id: "copy/atomic-file",
        },
        SyncFaultCase {
            fault: MemoryFault::DurableFileCopyNonDurable,
            phase: FileSystemContract::Copy,
            check_id: "copy/durable-file",
        },
        SyncFaultCase {
            fault: MemoryFault::DurableRenameNonDurable,
            phase: FileSystemContract::Rename,
            check_id: "rename/durable",
        },
        SyncFaultCase {
            fault: MemoryFault::AtomicTreeCopyNonAtomic,
            phase: FileSystemContract::Copy,
            check_id: "copy/atomic-tree",
        },
        SyncFaultCase {
            fault: MemoryFault::DurableTreeCopyNonDurable,
            phase: FileSystemContract::Copy,
            check_id: "copy/durable-tree",
        },
        SyncFaultCase {
            fault: MemoryFault::TreeCopyWrongStats,
            phase: FileSystemContract::Copy,
            check_id: "copy/atomic-tree",
        },
        SyncFaultCase {
            fault: MemoryFault::AtomicTempPersistNonAtomic,
            phase: FileSystemContract::TempResources,
            check_id: "temp/atomic",
        },
        SyncFaultCase {
            fault: MemoryFault::ServerSideCopyFallsBack,
            phase: FileSystemContract::Copy,
            check_id: "copy/server-side",
        },
        SyncFaultCase {
            fault: MemoryFault::ServerSideCopyFails,
            phase: FileSystemContract::Copy,
            check_id: "copy/server-side",
        },
        SyncFaultCase {
            fault: MemoryFault::IgnoreReadIfMatch,
            phase: FileSystemContract::Read,
            check_id: "read/if-match-stale",
        },
        SyncFaultCase {
            fault: MemoryFault::IgnoreReadIfNoneMatch,
            phase: FileSystemContract::Read,
            check_id: "read/if-none-match-current",
        },
        SyncFaultCase {
            fault: MemoryFault::IgnoreWriteIfMatch,
            phase: FileSystemContract::Write,
            check_id: "write/if-match",
        },
        SyncFaultCase {
            fault: MemoryFault::IgnoreDeleteIfMatch,
            phase: FileSystemContract::Delete,
            check_id: "delete/if-match",
        },
        SyncFaultCase {
            fault: MemoryFault::AtomicReplaceKeepsOldBytes,
            phase: FileSystemContract::Write,
            check_id: "write/atomic-replace-existing",
        },
        SyncFaultCase {
            fault: MemoryFault::DurableWriteDropsBytes,
            phase: FileSystemContract::Write,
            check_id: "write/durable",
        },
        SyncFaultCase {
            fault: MemoryFault::ChecksumIgnoresCorruption,
            phase: FileSystemContract::Read,
            check_id: "read/checksum-corruption",
        },
    ]
}

#[cfg(feature = "async")]
use super::async_memory_file_system::AsyncMemoryFault;

/// One asynchronous provider fault and the contract check that must expose it.
#[cfg(feature = "async")]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct AsyncFaultCase {
    /// The injected provider defect.
    pub fault: AsyncMemoryFault,
    /// The independently executable phase that owns the defect.
    pub phase: FileSystemContract,
    /// The stable check ID expected in the failure diagnostics.
    pub check_id: &'static str,
}

/// Returns the complete asynchronous fault-to-check matrix.
#[cfg(feature = "async")]
#[allow(dead_code)]
pub const fn async_fault_cases() -> &'static [AsyncFaultCase] {
    &[
        AsyncFaultCase {
            fault: AsyncMemoryFault::MissingPathExists,
            phase: FileSystemContract::ErrorContext,
            check_id: "error/context",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::WrongStatMetadata,
            phase: FileSystemContract::Stat,
            check_id: "stat/file-kind",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::CleanupStatError,
            phase: FileSystemContract::CreateDirectory,
            check_id: "directory/create",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::ReadWrongBytes,
            phase: FileSystemContract::Read,
            check_id: "read/basic",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::ReadFails,
            phase: FileSystemContract::Read,
            check_id: "read/basic",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::CreateDirectoryFails,
            phase: FileSystemContract::CreateDirectory,
            check_id: "directory/create",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::WriteDropsBytes,
            phase: FileSystemContract::Write,
            check_id: "write/basic",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::WriteFails,
            phase: FileSystemContract::Write,
            check_id: "write/basic",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::RecursiveCreateLeavesParentsMissing,
            phase: FileSystemContract::CreateDirectory,
            check_id: "directory/recursive",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::ListEscapesNamespace,
            phase: FileSystemContract::List,
            check_id: "list/basic",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::EmptyList,
            phase: FileSystemContract::List,
            check_id: "list/basic",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::ListDropsMetadata,
            phase: FileSystemContract::List,
            check_id: "list/prefix",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::DeleteNoOp,
            phase: FileSystemContract::Delete,
            check_id: "delete/basic",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::IgnoreDeleteIfMatch,
            phase: FileSystemContract::Delete,
            check_id: "delete/if-match",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::CopyDropsTarget,
            phase: FileSystemContract::Copy,
            check_id: "copy/basic",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::RenameNoOp,
            phase: FileSystemContract::Rename,
            check_id: "rename/basic",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::RenameWrongOutcome,
            phase: FileSystemContract::Rename,
            check_id: "rename/basic",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::DirectoryCopyDropsChildren,
            phase: FileSystemContract::Copy,
            check_id: "copy/atomic-tree",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::CopyOverwriteKeepsTarget,
            phase: FileSystemContract::Copy,
            check_id: "copy/fallback-overwrite-rejected",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::CopyAbortPublishes,
            phase: FileSystemContract::Copy,
            check_id: "async-copy/cancel-writer",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::CopyWriteFails,
            phase: FileSystemContract::Copy,
            check_id: "copy/basic",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::ServerSideCopyFallsBack,
            phase: FileSystemContract::Copy,
            check_id: "copy/server-side",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::ServerSideCopyFails,
            phase: FileSystemContract::Copy,
            check_id: "copy/server-side",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::CopyCancelAcknowledgeFails,
            phase: FileSystemContract::Copy,
            check_id: "async-copy/cancel-native-attempt",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::CopyCancelDisarmFails,
            phase: FileSystemContract::Copy,
            check_id: "async-copy/cancel-native-attempt",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::CopyCancelFailsBeforeStage,
            phase: FileSystemContract::Copy,
            check_id: "async-copy/cancel-native-attempt",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::TempCleanupNoOp,
            phase: FileSystemContract::TempResources,
            check_id: "temp/file",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::AppendOverwrites,
            phase: FileSystemContract::Write,
            check_id: "append/basic",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::CreateNewOverwrites,
            phase: FileSystemContract::Write,
            check_id: "write/create-conflict",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::ReplaceKeepsSuffix,
            phase: FileSystemContract::Write,
            check_id: "write/replace",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::AbortFailsOnce,
            phase: FileSystemContract::Write,
            check_id: "write/abort",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::AbortLies,
            phase: FileSystemContract::Write,
            check_id: "write/abort",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::ConditionalAbortReportsPublished,
            phase: FileSystemContract::Write,
            check_id: "write/if-absent",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::WriteAbortPublishes,
            phase: FileSystemContract::Write,
            check_id: "write/cancel-write",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::RecoveryAbortFailsOnce,
            phase: FileSystemContract::Write,
            check_id: "write/cancel-write",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::BasicCommitFails,
            phase: FileSystemContract::Write,
            check_id: "write/basic",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::OwningCommitFails,
            phase: FileSystemContract::Write,
            check_id: "write/owning-operation",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::ReplaceCommitFails,
            phase: FileSystemContract::Write,
            check_id: "write/replace",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::RecursiveDeleteLeavesChildren,
            phase: FileSystemContract::Delete,
            check_id: "delete/tree",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::AtomicRenameNonAtomic,
            phase: FileSystemContract::Rename,
            check_id: "rename/atomic",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::AtomicReplaceNonAtomic,
            phase: FileSystemContract::Write,
            check_id: "write/atomic-replace-existing",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::AtomicFileCopyNonAtomic,
            phase: FileSystemContract::Copy,
            check_id: "copy/atomic-file",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::AtomicTreeCopyNonAtomic,
            phase: FileSystemContract::Copy,
            check_id: "copy/atomic-tree",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::DurableTreeCopyNonDurable,
            phase: FileSystemContract::Copy,
            check_id: "copy/durable-tree",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::TreeCopyWrongStats,
            phase: FileSystemContract::Copy,
            check_id: "copy/atomic-tree",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::DurableFileCopyNonDurable,
            phase: FileSystemContract::Copy,
            check_id: "copy/durable-file",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::DurableRenameNonDurable,
            phase: FileSystemContract::Rename,
            check_id: "rename/durable",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::TempPersistWrongTarget,
            phase: FileSystemContract::TempResources,
            check_id: "temp/file",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::AtomicTempPersistNonAtomic,
            phase: FileSystemContract::TempResources,
            check_id: "temp/atomic",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::TempIgnoresOptions,
            phase: FileSystemContract::TempResources,
            check_id: "temp/file",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::TempCreationFails,
            phase: FileSystemContract::TempResources,
            check_id: "temp/file",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::TempKeepFails,
            phase: FileSystemContract::TempResources,
            check_id: "temp/file",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::TempPersistFails,
            phase: FileSystemContract::TempResources,
            check_id: "temp/atomic",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::TempCleanupFailsOnce,
            phase: FileSystemContract::TempResources,
            check_id: "temp/file",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::WriteCancelObserveFails,
            phase: FileSystemContract::Write,
            check_id: "write/cancel-open",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::WriteCancelAcknowledgeFails,
            phase: FileSystemContract::Write,
            check_id: "write/cancel-open",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::WriteCancelAcceptedBytesFails,
            phase: FileSystemContract::Write,
            check_id: "write/cancel-open",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::WriteCancelDisarmFails,
            phase: FileSystemContract::Write,
            check_id: "write/cancel-open",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::IgnoreReadIfMatch,
            phase: FileSystemContract::Read,
            check_id: "read/if-match-stale",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::IgnoreReadIfNoneMatch,
            phase: FileSystemContract::Read,
            check_id: "read/if-none-match-current",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::IgnoreWriteIfMatch,
            phase: FileSystemContract::Write,
            check_id: "write/if-match",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::AtomicReplaceKeepsOldBytes,
            phase: FileSystemContract::Write,
            check_id: "write/atomic-replace-existing",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::DurableWriteDropsBytes,
            phase: FileSystemContract::Write,
            check_id: "write/durable",
        },
        AsyncFaultCase {
            fault: AsyncMemoryFault::ChecksumIgnoresCorruption,
            phase: FileSystemContract::Read,
            check_id: "read/checksum-corruption",
        },
    ]
}

/// Extracts a panic payload into a stable searchable message.
#[allow(dead_code)]
pub fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|value| (*value).to_owned()))
        .unwrap_or_else(|| "<non-string panic payload>".to_owned())
}

/// Runs a phase and requires a panic naming its owning check ID.
#[allow(dead_code)]
pub fn assert_panics_at<F>(run: F, check_id: &str)
where
    F: FnOnce(),
{
    let result = std::panic::catch_unwind(AssertUnwindSafe(run));
    let payload = result.expect_err("faulty contract unexpectedly passed");
    let message = panic_message(payload);
    assert!(
        message.contains(check_id),
        "panic did not identify {check_id}: {message}"
    );
}
