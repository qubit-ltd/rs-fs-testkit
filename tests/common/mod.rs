// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use self::internal::memory_file_system;

#[cfg(feature = "async")]
pub(crate) mod async_memory_file_system;
pub(crate) mod check_matrix;
mod internal;
pub(crate) mod shared_model;

#[cfg(feature = "async")]
mod async_memory_write_cancellation_probe;
#[cfg(feature = "async")]
#[cfg(feature = "async")]
mod write_gate;

mod unavailable_scenario;

#[allow(unused)]
#[cfg(feature = "async")]
pub(crate) use async_memory_file_system::AsyncMemoryFault;
#[allow(unused)]
#[cfg(feature = "async")]
pub(crate) use async_memory_file_system::AsyncMemoryFixture;
#[allow(unused)]
pub use memory_file_system::MemoryFault;
#[allow(unused)]
pub use memory_file_system::MemoryFixture;
pub use unavailable_scenario::UnavailableScenario;
