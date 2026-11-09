// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Tests for crate-private internal contracts.

mod check_catalog_tests;
#[cfg(feature = "async")]
mod expected_write_cleanup_guard_tests;
mod finish_expected_write_failure_tests;
#[cfg(feature = "async")]
mod probe_disarm_guard_tests;
mod property_expectations_tests;
mod read_expectations_tests;
