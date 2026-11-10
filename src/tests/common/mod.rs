// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! In-memory fixtures used by crate-internal contract tests.

#[path = "../../../tests/common/internal/mod.rs"]
mod internal;
use self::internal::memory_file_system;

#[cfg(feature = "async")]
#[path = "../../../tests/common/async_memory_file_system.rs"]
pub(crate) mod async_memory_file_system;
#[path = "../../../tests/common/shared_model.rs"]
pub(crate) mod shared_model;

#[cfg(feature = "async")]
#[path = "../../../tests/common/async_memory_write_cancellation_probe.rs"]
mod async_memory_write_cancellation_probe;
#[path = "../../../tests/common/unavailable_scenario.rs"]
mod unavailable_scenario;
#[cfg(feature = "async")]
#[path = "../../../tests/common/write_gate.rs"]
mod write_gate;

#[cfg(feature = "async")]
pub(crate) use async_memory_file_system::AsyncMemoryFault;
#[cfg(feature = "async")]
pub(crate) use async_memory_file_system::AsyncMemoryFixture;
pub(crate) use memory_file_system::MemoryFault;
pub(crate) use memory_file_system::MemoryFixture;
pub(crate) use unavailable_scenario::UnavailableScenario;
