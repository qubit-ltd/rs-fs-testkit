// =============================================================================

#![cfg(feature = "async")]
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_fs_testkit::AsyncCopyCancellationStage;

/// Every asynchronous cancellation stage remains a distinct domain value.
#[test]
fn test_async_copy_cancellation_stages_are_distinct() {
    let stages = [
        AsyncCopyCancellationStage::NativeAttempt,
        AsyncCopyCancellationStage::Reader,
        AsyncCopyCancellationStage::Writer,
        AsyncCopyCancellationStage::Commit,
    ];

    for (index, stage) in stages.iter().enumerate() {
        assert_eq!(
            stages
                .iter()
                .filter(|candidate| *candidate == stage)
                .count(),
            1,
            "stage at index {index} must be unique",
        );
    }
}
