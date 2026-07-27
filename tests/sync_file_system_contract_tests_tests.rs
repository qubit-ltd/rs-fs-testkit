// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

qubit_fs_testkit::sync_file_system_contract_tests!(generated, super::common::MemoryFixture::new(),);
