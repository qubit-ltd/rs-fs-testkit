// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Pure request and byte expectations shared by both read drivers.

use qubit_fs::error::FsErrorKind;
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::metadata::ResourceVersion;
use qubit_fs::read::ChecksumPolicy;
use qubit_fs::read::ReadOptions;

use crate::ReadScenario;

/// Stable content prepared independently for positive reads.
pub(crate) const CONTENT: &[u8] = b"read contract bytes";

/// Builds requests without accepting weaker expectations from a fixture.
pub(crate) fn options(scenario: ReadScenario, limit: FileSystemLimit, version: ResourceVersion) -> ReadOptions {
    match scenario {
        ReadScenario::Basic | ReadScenario::RangeLimit => ReadOptions::default(),
        ReadScenario::Range => {
            let (offset, length) = window(limit);
            ReadOptions::default()
                .with_offset(Some(offset))
                .with_length(Some(length))
        }
        ReadScenario::IfMatchCurrent | ReadScenario::IfMatchStale => {
            ReadOptions::default().with_if_match(Some(version))
        }
        ReadScenario::IfNoneMatchCurrent | ReadScenario::IfNoneMatchStale => {
            ReadOptions::default().with_if_none_match(Some(version))
        }
        ReadScenario::Checksum | ReadScenario::ChecksumCorruption => {
            ReadOptions::default().with_checksum(ChecksumPolicy::Required)
        }
    }
}

/// Returns the exact independently seeded bytes required from a positive read.
pub(crate) fn bytes(scenario: ReadScenario, limit: FileSystemLimit) -> &'static [u8] {
    if scenario == ReadScenario::Range {
        let (offset, length) = window(limit);
        &CONTENT[offset as usize..(offset + length) as usize]
    } else {
        CONTENT
    }
}

/// Distinguishes deliberate rejection scenarios from successful reads.
pub(crate) const fn rejection(scenario: ReadScenario) -> Option<FsErrorKind> {
    match scenario {
        ReadScenario::IfMatchStale | ReadScenario::IfNoneMatchCurrent => Some(FsErrorKind::PreconditionFailed),
        ReadScenario::ChecksumCorruption => Some(FsErrorKind::DataCorruption),
        _ => None,
    }
}

/// Reports whether this request needs independently observed version evidence.
pub(crate) const fn uses_version(scenario: ReadScenario) -> bool {
    matches!(
        scenario,
        ReadScenario::IfMatchCurrent
            | ReadScenario::IfMatchStale
            | ReadScenario::IfNoneMatchCurrent
            | ReadScenario::IfNoneMatchStale
    )
}

/// Reports whether the request excludes or requires a deliberately stale
/// version.
pub(crate) const fn uses_stale(scenario: ReadScenario) -> bool {
    matches!(scenario, ReadScenario::IfMatchStale | ReadScenario::IfNoneMatchStale)
}

/// Bounds the range by both provider limits and the known seed length.
fn window(limit: FileSystemLimit) -> (u64, u64) {
    let length = limit.maximum().map_or(8, |maximum| maximum.min(8));
    (if length >= 8 { 5 } else { 0 }, length)
}
