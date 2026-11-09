// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Crate-internal tests organized by their production modules.

pub(crate) mod common;
mod contract_context_tests;
mod contract_failure_tests;
mod contract_report_tests;
mod contract_source_tests;
mod contract_temp_failure_tests;
mod contract_writer_failure_tests;
mod fs_rustdoc_support;
mod internal;
mod rustdoc_support;
mod suite_unit_build_coverage_tests;
