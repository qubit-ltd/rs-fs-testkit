// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// Keep executable Rustdoc examples on the same packaged fixture used by unit
// tests.
#[path = "../../src/tests/rustdoc_support.rs"]
mod shared_support;

#[cfg(feature = "async")]
pub use shared_support::RustdocAsyncFixture;
pub use shared_support::RustdocSyncFixture;
pub use shared_support::fs_rustdoc_support;
