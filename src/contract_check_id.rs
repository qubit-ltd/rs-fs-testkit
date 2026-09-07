// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Exhaustive typed identities for provider contract evidence.

use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;

/// Identifies one independently reported provider contract check.
///
/// Stable names are intended for test selection and diagnostics. A report
/// cannot invent a name outside this set; execution and evidence use the same
/// identity even when synchronous and asynchronous drivers differ.
///
/// # Examples
///
/// ```
/// use qubit_fs_testkit::ContractCheckId;
/// let check = ContractCheckId::WriteCancelOpen;
/// assert_eq!(check.as_str(), "write/cancel-open");
/// assert!(ContractCheckId::ALL.contains(&check));
/// ```
#[must_use]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ContractCheckId {
    /// Evidence for the `append/basic` contract.
    AppendBasic,
    /// Evidence for the `async-copy/cancel-commit` contract.
    AsyncCopyCancelCommit,
    /// Evidence for the `async-copy/cancel-native-attempt` contract.
    AsyncCopyCancelNativeAttempt,
    /// Evidence for the `async-copy/cancel-reader` contract.
    AsyncCopyCancelReader,
    /// Evidence for the `async-copy/cancel-writer` contract.
    AsyncCopyCancelWriter,
    /// Evidence for the `copy/atomic-file` contract.
    CopyAtomicFile,
    /// Evidence for the `copy/atomic-tree` contract.
    CopyAtomicTree,
    /// Evidence for the `copy/basic` contract.
    CopyBasic,
    /// Evidence for the `copy/durable-file` contract.
    CopyDurableFile,
    /// Evidence for the `copy/durable-tree` contract.
    CopyDurableTree,
    /// Evidence for the `copy/fallback-overwrite-rejected` contract.
    CopyFallbackOverwriteRejected,
    /// Evidence for the `copy/repeated-execute` contract.
    CopyRepeatedExecute,
    /// Evidence for the `copy/server-side` contract.
    CopyServerSide,
    /// Evidence for the `delete/basic` contract.
    DeleteBasic,
    /// Evidence for the `delete/if-match` contract.
    DeleteIfMatch,
    /// Evidence for the `delete/missing-ok` contract.
    DeleteMissingOk,
    /// Evidence for the `delete/tree` contract.
    DeleteTree,
    /// Evidence for the `directory/create` contract.
    DirectoryCreate,
    /// Evidence for the `directory/recursive` contract.
    DirectoryRecursive,
    /// Evidence for the `error/context` contract.
    ErrorContext,
    /// Evidence for the `list/basic` contract.
    ListBasic,
    /// Evidence for the `list/literal-prefix` contract.
    ListLiteralPrefix,
    /// Evidence for the `list/pagination` contract.
    ListPagination,
    /// Evidence for the `list/prefix` contract.
    ListPrefix,
    /// Evidence for the `properties/capability-dependencies` contract.
    PropertiesCapabilityDependencies,
    /// Evidence for the `properties/limit-component-admission` contract.
    PropertiesLimitComponentAdmission,
    /// Evidence for the `properties/limit-list-page` contract.
    PropertiesLimitListPage,
    /// Evidence for the `properties/limit-path-admission` contract.
    PropertiesLimitPathAdmission,
    /// Evidence for the `properties/limits` contract.
    PropertiesLimits,
    /// Evidence for the `properties/path-constraints` contract.
    PropertiesPathConstraints,
    /// Evidence for the `properties/snapshot` contract.
    PropertiesSnapshot,
    /// Evidence for the `properties/symlink-policy` contract.
    PropertiesSymlinkPolicy,
    /// Evidence for the `read/basic` contract.
    ReadBasic,
    /// Evidence for the `read/checksum` contract.
    ReadChecksum,
    /// Evidence for the `read/checksum-corruption` contract.
    ReadChecksumCorruption,
    /// Evidence for the `read/if-match-current` contract.
    ReadIfMatchCurrent,
    /// Evidence for the `read/if-match-stale` contract.
    ReadIfMatchStale,
    /// Evidence for the `read/if-none-match-current` contract.
    ReadIfNoneMatchCurrent,
    /// Evidence for the `read/if-none-match-stale` contract.
    ReadIfNoneMatchStale,
    /// Evidence for the `read/range` contract.
    ReadRange,
    /// Evidence for the `read/range-limit` contract.
    ReadRangeLimit,
    /// Evidence for the `rename/atomic` contract.
    RenameAtomic,
    /// Evidence for the `rename/basic` contract.
    RenameBasic,
    /// Evidence for the `rename/conflict` contract.
    RenameConflict,
    /// Evidence for the `rename/durable` contract.
    RenameDurable,
    /// Evidence for the `representation/empty` contract.
    RepresentationEmpty,
    /// Evidence for the `representation/symlink` contract.
    RepresentationSymlink,
    /// Evidence for the `stat/basic` contract.
    StatBasic,
    /// Evidence for the `stat/file-kind` contract.
    StatFileKind,
    /// Evidence for the `temp/atomic` contract.
    TempAtomic,
    /// Evidence for the `temp/directory` contract.
    TempDirectory,
    /// Evidence for the `temp/file` contract.
    TempFile,
    /// Evidence for the `temp/repeated-lifecycle` contract.
    TempRepeatedLifecycle,
    /// Evidence for the `write/atomic-replace-existing` contract.
    WriteAtomicReplaceExisting,
    /// Evidence for the `write/basic` contract.
    WriteBasic,
    /// Evidence for the `write/create-conflict` contract.
    WriteCreateConflict,
    /// Evidence for the `write/replace` contract.
    WriteReplace,
    /// Evidence for the `write/abort` contract.
    WriteAbort,
    /// Evidence for the `write/cancel-commit` contract.
    WriteCancelCommit,
    /// Evidence for the `write/cancel-flush` contract.
    WriteCancelFlush,
    /// Evidence for the `write/cancel-open` contract.
    WriteCancelOpen,
    /// Evidence for the `write/cancel-write` contract.
    WriteCancelWrite,
    /// Evidence for the `write/durable` contract.
    WriteDurable,
    /// Evidence for the `write/if-absent` contract.
    WriteIfAbsent,
    /// Evidence for the `write/if-match` contract.
    WriteIfMatch,
    /// Evidence for the `write/limit` contract.
    WriteLimit,
    /// Evidence for the `write/owning-operation` contract.
    WriteOwningOperation,
    /// Evidence for the `write/repeated-execute` contract.
    WriteRepeatedExecute,
}

impl ContractCheckId {
    /// Every recognized identity, including runtime-specific optional probes.
    pub const ALL: &[Self] = &[
        Self::AppendBasic,
        Self::AsyncCopyCancelCommit,
        Self::AsyncCopyCancelNativeAttempt,
        Self::AsyncCopyCancelReader,
        Self::AsyncCopyCancelWriter,
        Self::CopyAtomicFile,
        Self::CopyAtomicTree,
        Self::CopyBasic,
        Self::CopyDurableFile,
        Self::CopyDurableTree,
        Self::CopyFallbackOverwriteRejected,
        Self::CopyRepeatedExecute,
        Self::CopyServerSide,
        Self::DeleteBasic,
        Self::DeleteIfMatch,
        Self::DeleteMissingOk,
        Self::DeleteTree,
        Self::DirectoryCreate,
        Self::DirectoryRecursive,
        Self::ErrorContext,
        Self::ListBasic,
        Self::ListLiteralPrefix,
        Self::ListPagination,
        Self::ListPrefix,
        Self::PropertiesCapabilityDependencies,
        Self::PropertiesLimitComponentAdmission,
        Self::PropertiesLimitListPage,
        Self::PropertiesLimitPathAdmission,
        Self::PropertiesLimits,
        Self::PropertiesPathConstraints,
        Self::PropertiesSnapshot,
        Self::PropertiesSymlinkPolicy,
        Self::ReadBasic,
        Self::ReadChecksum,
        Self::ReadChecksumCorruption,
        Self::ReadIfMatchCurrent,
        Self::ReadIfMatchStale,
        Self::ReadIfNoneMatchCurrent,
        Self::ReadIfNoneMatchStale,
        Self::ReadRange,
        Self::ReadRangeLimit,
        Self::RenameAtomic,
        Self::RenameBasic,
        Self::RenameConflict,
        Self::RenameDurable,
        Self::RepresentationEmpty,
        Self::RepresentationSymlink,
        Self::StatBasic,
        Self::StatFileKind,
        Self::TempAtomic,
        Self::TempDirectory,
        Self::TempFile,
        Self::TempRepeatedLifecycle,
        Self::WriteAtomicReplaceExisting,
        Self::WriteBasic,
        Self::WriteCreateConflict,
        Self::WriteReplace,
        Self::WriteAbort,
        Self::WriteCancelCommit,
        Self::WriteCancelFlush,
        Self::WriteCancelOpen,
        Self::WriteCancelWrite,
        Self::WriteDurable,
        Self::WriteIfAbsent,
        Self::WriteIfMatch,
        Self::WriteLimit,
        Self::WriteOwningOperation,
        Self::WriteRepeatedExecute,
    ];

    /// Returns the sole phase responsible for this check.
    #[must_use]
    pub const fn contract(self) -> crate::FileSystemContract {
        crate::internal::check_catalog::specification(self).contract
    }

    /// Returns whether missing instrumentation may skip this diagnostic probe.
    ///
    /// Executed failures are never optional. Callers can require these probes
    /// through the report's explicit requirement selection.
    #[must_use]
    pub const fn is_optional(self) -> bool {
        crate::internal::check_catalog::specification(self).optional
    }

    /// Returns whether the synchronous driver can execute this check.
    #[must_use]
    pub const fn supports_synchronous(self) -> bool {
        !crate::internal::check_catalog::specification(self).asynchronous_only
    }

    /// Returns this check's allocation-free stable diagnostic name.
    ///
    /// # Returns
    ///
    /// A process-lifetime name shared by selection, reports and diagnostics.
    #[must_use]
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AppendBasic => "append/basic",
            Self::AsyncCopyCancelCommit => "async-copy/cancel-commit",
            Self::AsyncCopyCancelNativeAttempt => "async-copy/cancel-native-attempt",
            Self::AsyncCopyCancelReader => "async-copy/cancel-reader",
            Self::AsyncCopyCancelWriter => "async-copy/cancel-writer",
            Self::CopyAtomicFile => "copy/atomic-file",
            Self::CopyAtomicTree => "copy/atomic-tree",
            Self::CopyBasic => "copy/basic",
            Self::CopyDurableFile => "copy/durable-file",
            Self::CopyDurableTree => "copy/durable-tree",
            Self::CopyFallbackOverwriteRejected => "copy/fallback-overwrite-rejected",
            Self::CopyRepeatedExecute => "copy/repeated-execute",
            Self::CopyServerSide => "copy/server-side",
            Self::DeleteBasic => "delete/basic",
            Self::DeleteIfMatch => "delete/if-match",
            Self::DeleteMissingOk => "delete/missing-ok",
            Self::DeleteTree => "delete/tree",
            Self::DirectoryCreate => "directory/create",
            Self::DirectoryRecursive => "directory/recursive",
            Self::ErrorContext => "error/context",
            Self::ListBasic => "list/basic",
            Self::ListLiteralPrefix => "list/literal-prefix",
            Self::ListPagination => "list/pagination",
            Self::ListPrefix => "list/prefix",
            Self::PropertiesCapabilityDependencies => "properties/capability-dependencies",
            Self::PropertiesLimitComponentAdmission => "properties/limit-component-admission",
            Self::PropertiesLimitListPage => "properties/limit-list-page",
            Self::PropertiesLimitPathAdmission => "properties/limit-path-admission",
            Self::PropertiesLimits => "properties/limits",
            Self::PropertiesPathConstraints => "properties/path-constraints",
            Self::PropertiesSnapshot => "properties/snapshot",
            Self::PropertiesSymlinkPolicy => "properties/symlink-policy",
            Self::ReadBasic => "read/basic",
            Self::ReadChecksum => "read/checksum",
            Self::ReadChecksumCorruption => "read/checksum-corruption",
            Self::ReadIfMatchCurrent => "read/if-match-current",
            Self::ReadIfMatchStale => "read/if-match-stale",
            Self::ReadIfNoneMatchCurrent => "read/if-none-match-current",
            Self::ReadIfNoneMatchStale => "read/if-none-match-stale",
            Self::ReadRange => "read/range",
            Self::ReadRangeLimit => "read/range-limit",
            Self::RenameAtomic => "rename/atomic",
            Self::RenameBasic => "rename/basic",
            Self::RenameConflict => "rename/conflict",
            Self::RenameDurable => "rename/durable",
            Self::RepresentationEmpty => "representation/empty",
            Self::RepresentationSymlink => "representation/symlink",
            Self::StatBasic => "stat/basic",
            Self::StatFileKind => "stat/file-kind",
            Self::TempAtomic => "temp/atomic",
            Self::TempDirectory => "temp/directory",
            Self::TempFile => "temp/file",
            Self::TempRepeatedLifecycle => "temp/repeated-lifecycle",
            Self::WriteAtomicReplaceExisting => "write/atomic-replace-existing",
            Self::WriteBasic => "write/basic",
            Self::WriteCreateConflict => "write/create-conflict",
            Self::WriteReplace => "write/replace",
            Self::WriteAbort => "write/abort",
            Self::WriteCancelCommit => "write/cancel-commit",
            Self::WriteCancelFlush => "write/cancel-flush",
            Self::WriteCancelOpen => "write/cancel-open",
            Self::WriteCancelWrite => "write/cancel-write",
            Self::WriteDurable => "write/durable",
            Self::WriteIfAbsent => "write/if-absent",
            Self::WriteIfMatch => "write/if-match",
            Self::WriteLimit => "write/limit",
            Self::WriteOwningOperation => "write/owning-operation",
            Self::WriteRepeatedExecute => "write/repeated-execute",
        }
    }
}

impl Display for ContractCheckId {
    /// Writes the stable name without allocating or exposing provider data.
    #[inline]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(self.as_str())
    }
}
