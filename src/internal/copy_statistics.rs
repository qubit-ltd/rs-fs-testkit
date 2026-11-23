// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Internal validation helpers for copy statistics.

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

#[cfg(test)]
mod tests {
    use super::copy_count_matches;

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
