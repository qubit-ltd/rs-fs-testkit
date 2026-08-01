// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Private helpers shared by filesystem contract assertions.

mod assertions;
mod catch_unwind_future;

pub(crate) use assertions::{
    assert_error_with_source_or_target,
    assert_error_with_target,
    assert_unsupported_error,
};
pub(crate) use catch_unwind_future::catch_unwind_future;
