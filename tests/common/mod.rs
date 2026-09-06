// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

#[cfg(feature = "async")]
mod async_memory_file_system;
mod memory_file_system;
mod shared_model;

#[allow(unused)]
#[cfg(feature = "async")]
pub use async_memory_file_system::AsyncMemoryFault;
#[allow(unused)]
#[cfg(feature = "async")]
pub use async_memory_file_system::AsyncMemoryFixture;
#[allow(unused)]
pub use memory_file_system::MemoryFault;
#[allow(unused)]
pub use memory_file_system::MemoryFixture;
