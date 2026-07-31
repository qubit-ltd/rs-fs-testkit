// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Named phases shared by synchronous and asynchronous contract suites.

/// One independently executable provider contract phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileSystemContract {
    /// Immutable facade properties and fixture path compatibility.
    Properties,
    /// Metadata identity and provider-neutral file kinds.
    Stat,
    /// Reads, ranges, conditions, and checksum validation.
    Read,
    /// Writes, dispositions, conditions, replacement, and abort.
    Write,
    /// Direct, prefixed, metadata-bearing, and paged listings.
    List,
    /// Directory creation and recursive ancestor creation.
    CreateDirectory,
    /// Empty-directory/prefix and symbolic-link representations.
    Representations,
    /// File, directory, conditional, missing-ok, and recursive deletion.
    Delete,
    /// Copy methods, conflicts, statistics, and guarantees.
    Copy,
    /// Rename identity, conflicts, overwrite, and guarantees.
    Rename,
    /// Append publication.
    Append,
    /// Recursive directory or prefix deletion.
    RecursiveDelete,
    /// Required atomic rename.
    AtomicRename,
    /// Required atomic replacement.
    AtomicReplace,
    /// Required durable copy.
    DurableCopy,
    /// Temporary file and directory lifecycle and persistence.
    TempResources,
    /// Structured public error context.
    ErrorContext,
}

impl FileSystemContract {
    /// Every named contract in dependency-safe execution order.
    pub const ALL: [Self; 17] = [
        Self::Properties,
        Self::Stat,
        Self::Read,
        Self::Write,
        Self::List,
        Self::CreateDirectory,
        Self::Representations,
        Self::Delete,
        Self::Copy,
        Self::Rename,
        Self::Append,
        Self::RecursiveDelete,
        Self::AtomicRename,
        Self::AtomicReplace,
        Self::DurableCopy,
        Self::TempResources,
        Self::ErrorContext,
    ];
}
