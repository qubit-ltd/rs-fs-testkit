// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

#[cfg(feature = "async")]
pub(crate) mod async_memory_file_system;
mod memory_file_system;
mod shared_model;

#[allow(unused)]
#[cfg(feature = "async")]
pub(crate) use async_memory_file_system::AsyncMemoryFault;
#[allow(unused)]
#[cfg(feature = "async")]
pub(crate) use async_memory_file_system::AsyncMemoryFixture;
#[cfg(feature = "async")]
#[allow(unused_imports)]
pub(crate) use async_memory_file_system::run_controlled;
#[allow(unused)]
pub use memory_file_system::MemoryFault;
#[allow(unused)]
pub use memory_file_system::MemoryFixture;
