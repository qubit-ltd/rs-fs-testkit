// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Independent SDK-backed S3 contract fixture.

mod fixture;

pub use fixture::Fixture;

#[allow(
    dead_code,
    reason = "shared test module also compiled by listing-only binaries"
)]
pub mod recovery_matrix;

pub mod http_support;
