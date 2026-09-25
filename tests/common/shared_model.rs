// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared in-memory provider data model used by sync and async fixtures.

#[derive(Clone)]
pub(crate) enum Entry {
    File(Vec<u8>),
    Directory,
    Symlink,
}
