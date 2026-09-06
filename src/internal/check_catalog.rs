// qubit-style: allow type-file-name
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Stable check catalog used to detect missing phase evidence.

use qubit_fs::metadata::FileSystemCapability;

use crate::FileSystemContract;

/// Description of one catalog entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CheckSpec {
    /// Stable check identifier.
    pub(crate) id: &'static str,
    /// Capability required by the check, if any.
    pub(crate) capability: Option<FileSystemCapability>,
}

const fn capability(capability: FileSystemCapability, id: &'static str) -> CheckSpec {
    CheckSpec {
        id,
        capability: Some(capability),
    }
}

const fn unscoped(id: &'static str) -> CheckSpec {
    CheckSpec { id, capability: None }
}

/// Returns the required check IDs for one independently run phase.
pub(crate) fn for_contract(contract: FileSystemContract) -> Vec<CheckSpec> {
    match contract {
        FileSystemContract::Properties => vec![
            capability(FileSystemCapability::Read, "properties/snapshot"),
            unscoped("properties/path-constraints"),
            unscoped("properties/capability-dependencies"),
            unscoped("properties/limits"),
            unscoped("properties/limit-path-admission"),
            unscoped("properties/symlink-policy"),
        ],
        FileSystemContract::Stat => vec![
            capability(FileSystemCapability::Read, "stat/basic"),
            unscoped("stat/file-kind"),
        ],
        FileSystemContract::Read => vec![
            capability(FileSystemCapability::Read, "read/basic"),
            capability(FileSystemCapability::RangeRead, "read/range"),
            capability(FileSystemCapability::RangeRead, "read/range-limit"),
            capability(FileSystemCapability::ConditionalRead, "read/if-match-current"),
            capability(FileSystemCapability::ConditionalRead, "read/if-match-stale"),
            capability(FileSystemCapability::ConditionalRead, "read/if-none-match-current"),
            capability(FileSystemCapability::ConditionalRead, "read/if-none-match-stale"),
            capability(FileSystemCapability::ChecksumValidation, "read/checksum"),
            capability(FileSystemCapability::ChecksumValidation, "read/checksum-corruption"),
        ],
        FileSystemContract::Write => vec![
            capability(FileSystemCapability::Write, "write/basic"),
            capability(FileSystemCapability::Write, "write/limit"),
            capability(FileSystemCapability::ConditionalWrite, "write/if-absent"),
            capability(FileSystemCapability::ConditionalWrite, "write/if-match"),
            capability(FileSystemCapability::AtomicReplace, "write/atomic-replace-existing"),
            capability(FileSystemCapability::DurableWrite, "write/durable"),
        ],
        FileSystemContract::List => vec![
            capability(FileSystemCapability::List, "list/basic"),
            capability(FileSystemCapability::List, "list/prefix"),
            capability(FileSystemCapability::List, "list/pagination"),
        ],
        FileSystemContract::CreateDirectory => vec![
            capability(FileSystemCapability::CreateDirectory, "directory/create"),
            capability(FileSystemCapability::CreateDirectory, "directory/recursive"),
        ],
        FileSystemContract::Representations => vec![
            capability(FileSystemCapability::EmptyDirectory, "representation/empty"),
            capability(FileSystemCapability::Symlink, "representation/symlink"),
        ],
        FileSystemContract::Delete => vec![
            capability(FileSystemCapability::Delete, "delete/basic"),
            capability(FileSystemCapability::Delete, "delete/missing-ok"),
            capability(FileSystemCapability::ConditionalDelete, "delete/if-match"),
        ],
        FileSystemContract::Copy => vec![
            capability(FileSystemCapability::Copy, "copy/basic"),
            capability(FileSystemCapability::Copy, "copy/fallback-overwrite-rejected"),
            capability(FileSystemCapability::ServerSideCopy, "copy/server-side"),
            capability(FileSystemCapability::AtomicFileCopy, "copy/atomic-file"),
            capability(FileSystemCapability::AtomicTreeCopy, "copy/atomic-tree"),
        ],
        FileSystemContract::Rename => vec![
            capability(FileSystemCapability::Rename, "rename/basic"),
            capability(FileSystemCapability::Rename, "rename/conflict"),
        ],
        FileSystemContract::Append => vec![capability(FileSystemCapability::Append, "append/basic")],
        FileSystemContract::RecursiveDelete => vec![capability(FileSystemCapability::RecursiveDelete, "delete/tree")],
        FileSystemContract::AtomicRename => vec![capability(FileSystemCapability::AtomicRename, "rename/atomic")],
        FileSystemContract::DurableRename => vec![capability(FileSystemCapability::DurableRename, "rename/durable")],
        FileSystemContract::AtomicReplace => vec![capability(
            FileSystemCapability::AtomicReplace,
            "write/atomic-replace-existing",
        )],
        FileSystemContract::DurableFileCopy => vec![
            capability(FileSystemCapability::DurableFileCopy, "copy/durable-file"),
            capability(FileSystemCapability::DurableTreeCopy, "copy/durable-tree"),
        ],
        FileSystemContract::TempResources => vec![
            capability(FileSystemCapability::TempFile, "temp/file"),
            capability(FileSystemCapability::TempDirectory, "temp/directory"),
            capability(FileSystemCapability::AtomicTempPersist, "temp/atomic"),
        ],
        FileSystemContract::ErrorContext => vec![unscoped("error/context")],
    }
}
