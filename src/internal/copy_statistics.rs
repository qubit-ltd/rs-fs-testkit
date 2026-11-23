// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Internal validation helpers for copy statistics.

use qubit_fs::copy::CopyStats;

/// Returns whether two copy counters add up to the expected total without
/// overflow.
///
/// # Parameters
///
/// * `left` - The first counter.
/// * `right` - The second counter.
/// * `expected` - The expected total.
pub(crate) fn copy_count_matches(left: u64, right: u64, expected: u64) -> bool {
    left.checked_add(right) == Some(expected)
}

/// Checks the statistics for one successful single-file copy request.
pub(crate) fn single_file_copy_matches(stats: &CopyStats, expected_bytes: u64) -> bool {
    copy_count_matches(stats.files, stats.objects, 1)
        && stats.bytes == expected_bytes
        && stats.skipped == 0
        && stats.failed == 0
        && stats.overwritten == 0
}

/// Checks the statistics for one skipped single-file copy request.
pub(crate) fn single_file_copy_skipped(stats: &CopyStats) -> bool {
    stats.files == 0
        && stats.objects == 0
        && stats.bytes == 0
        && stats.skipped == 1
        && stats.failed == 0
        && stats.overwritten == 0
}

/// Checks the statistics for one overwritten single-file copy request.
pub(crate) fn single_file_copy_overwritten(stats: &CopyStats, expected_bytes: u64) -> bool {
    copy_count_matches(stats.files, stats.objects, 1)
        && stats.bytes == expected_bytes
        && stats.skipped == 0
        && stats.failed == 0
        && stats.overwritten == 1
}

#[cfg(test)]
mod tests {
    use super::copy_count_matches;

    #[test]
    fn test_single_file_copy_scenarios_reject_contradictory_counters() {
        use super::single_file_copy_matches;
        use super::single_file_copy_overwritten;
        use super::single_file_copy_skipped;
        use qubit_fs::copy::CopyStats;

        assert!(single_file_copy_matches(
            &CopyStats { files: 1, bytes: 10, ..CopyStats::default() },
            10,
        ));
        for invalid in [
            CopyStats { files: 1, bytes: 10, skipped: 1, ..CopyStats::default() },
            CopyStats { files: 1, bytes: 10, failed: 1, ..CopyStats::default() },
            CopyStats { files: 1, bytes: 10, overwritten: 1, ..CopyStats::default() },
        ] {
            assert!(!single_file_copy_matches(&invalid, 10));
        }

        assert!(single_file_copy_skipped(&CopyStats { skipped: 1, ..CopyStats::default() }));
        assert!(!single_file_copy_skipped(&CopyStats { files: 1, skipped: 1, ..CopyStats::default() }));

        assert!(single_file_copy_overwritten(
            &CopyStats { files: 1, bytes: 10, overwritten: 1, ..CopyStats::default() },
            10,
        ));
        assert!(!single_file_copy_overwritten(
            &CopyStats { files: 1, bytes: 10, overwritten: 1, failed: 1, ..CopyStats::default() },
            10,
        ));
    }

    #[test]
    fn test_accepts_valid_totals() {
        assert!(copy_count_matches(1, 0, 1));
        assert!(copy_count_matches(1, 1, 2));
    }

    #[test]
    fn test_rejects_overflowing_file_and_tree_totals() {
        assert!(!copy_count_matches(u64::MAX, 2, 1)); // files + objects
        assert!(!copy_count_matches(u64::MAX, 2, 2)); // directories + prefixes
    }
}
