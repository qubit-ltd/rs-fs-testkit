// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Private helpers shared by filesystem contract assertions.

mod assertions;
#[cfg(feature = "async")]
mod catch_unwind_future;
pub(crate) mod check_catalog;
pub(crate) mod check_spec;
pub(crate) mod cleanup_failure;
#[cfg(feature = "async")]
pub(crate) mod execute_copy;
#[cfg(feature = "async")]
pub(crate) mod execute_write;
pub(crate) mod limit_probe_plan;
#[cfg(feature = "async")]
pub(crate) mod probe_disarm_guard;
pub(crate) mod property_expectations;
pub(crate) mod read_expectations;
pub(crate) mod tracked_resource;

pub(crate) use assertions::assert_error_with_source_or_target;
pub(crate) use assertions::assert_error_with_target;
pub(crate) use assertions::assert_unsupported_error;
pub(crate) use assertions::verify_condition;
pub(crate) use assertions::verify_fs_error;
pub(crate) use assertions::verify_missing_error;
#[cfg(feature = "async")]
pub(crate) use catch_unwind_future::catch_unwind_future;

mod verify_open_failure;
pub(crate) use verify_open_failure::verify_open_failure;

pub(crate) mod finish_expected_write_failure;

#[cfg(feature = "async")]
pub(crate) mod expected_write_cleanup_guard;
