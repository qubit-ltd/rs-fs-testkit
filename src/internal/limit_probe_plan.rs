// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Bounded, I/O-free plans for finite resource-limit probes.

use qubit_fs::metadata::FileSystemLimit;

/// Selects a fixed sample prefix that fits a declared write maximum.
///
/// Unknown and non-finite limits retain the complete sample. The returned
/// slice borrows `sample` and does not allocate.
#[inline]
pub(crate) fn bounded_payload(sample: &[u8], limit: FileSystemLimit) -> &[u8] {
    match limit {
        FileSystemLimit::Maximum(maximum) => {
            let maximum = usize::try_from(maximum).unwrap_or(usize::MAX);
            &sample[..sample.len().min(maximum)]
        }
        FileSystemLimit::Unknown | FileSystemLimit::NotApplicable | FileSystemLimit::Unbounded => sample,
    }
}

/// Maximum payload allocated by one contract limit probe.
pub(crate) const MAX_PROBE_BYTES: u64 = 64 * 1024;
/// Maximum page-entry hint used by bounded list probes.
pub(crate) const MAX_PROBE_ENTRIES: u64 = 64;
/// Returns the boundary value and its checked successor when within budget.
#[inline]
pub(crate) const fn bounded_successor(maximum: u64, budget: u64) -> Option<(u64, u64)> {
    if maximum >= budget {
        return None;
    }
    match maximum.checked_add(1) {
        Some(successor) if successor <= budget => Some((maximum, successor)),
        _ => None,
    }
}

/// Plans a finite limit probe while keeping allocations within the testkit
/// budget. Non-finite declarations are intentionally left unprobed.
#[inline]
pub(crate) const fn finite_probe(limit: FileSystemLimit, budget: u64) -> Option<(u64, u64)> {
    match limit {
        FileSystemLimit::Maximum(maximum) => bounded_successor(maximum, budget),
        FileSystemLimit::Unknown | FileSystemLimit::NotApplicable | FileSystemLimit::Unbounded => None,
    }
}
