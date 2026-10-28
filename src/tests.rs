// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Unit tests that exercise crate-private contracts through their narrowest
//! stable boundary.

#[path = "tests/contract_context_tests.rs"]
mod contract_context_tests;
#[path = "tests/contract_failure_tests.rs"]
mod contract_failure_tests;
#[path = "tests/contract_report_tests.rs"]
mod contract_report_tests;
#[path = "tests/contract_source_tests.rs"]
mod contract_source_tests;
#[path = "tests/contract_temp_failure_tests.rs"]
mod contract_temp_failure_tests;
#[path = "tests/contract_writer_failure_tests.rs"]
mod contract_writer_failure_tests;
#[path = "tests/internal/mod.rs"]
mod internal;
#[path = "tests/suite_unit_build_coverage_tests.rs"]
mod suite_unit_build_coverage_tests;
