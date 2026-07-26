// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use common::MemoryFixture;

/// Verifies unsupported operations expose structured capability errors.
#[test]
fn test_unsupported_operations_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_unsupported_operations_contract(&MemoryFixture::new());
}
