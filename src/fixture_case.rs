// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Fixture-level applicability declarations for contract probes.

use qubit_fs::metadata::FileSystemCapability;

/// Identifies a provider scenario that a fixture can prepare and observe.
#[must_use]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureCase {
    /// A basic capability or guarantee probe.
    Capability(FileSystemCapability),
    /// A file copy whose destination already exists.
    CopyOverwrite,
    /// A directory-tree copy probe.
    CopyTree,
    /// A read conditional on a resource version.
    ReadIfMatch,
    /// A read conditional on resource absence or a different version.
    ReadIfNoneMatch,
    /// A write conditional on destination absence.
    WriteIfAbsent,
    /// A write conditional on a current resource version.
    WriteIfMatch,
    /// A delete conditional on a resource version.
    DeleteIfMatch,
}
