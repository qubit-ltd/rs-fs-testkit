// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! In-memory fixtures used by crate-internal contract tests.

use self::internal::memory_file_system;

#[cfg(feature = "async")]
pub(crate) mod async_memory_file_system;
mod internal;
pub(crate) mod shared_model;

#[cfg(feature = "async")]
mod async_memory_write_cancellation_probe;
mod unavailable_scenario;
#[cfg(feature = "async")]
mod write_gate;

#[cfg(feature = "async")]
pub(crate) use async_memory_file_system::AsyncMemoryFault;
#[cfg(feature = "async")]
pub(crate) use async_memory_file_system::AsyncMemoryFixture;
pub(crate) use memory_file_system::MemoryFault;
pub(crate) use memory_file_system::MemoryFixture;
pub(crate) use unavailable_scenario::UnavailableScenario;
