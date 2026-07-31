// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================

use qubit_fs_testkit::FileSystemContract;

/// The named phase list remains complete and dependency ordered.
#[test]
fn test_file_system_contract_all_contains_each_phase_once() {
    assert_eq!(17, FileSystemContract::ALL.len());
    for (index, contract) in FileSystemContract::ALL.iter().enumerate() {
        assert!(!FileSystemContract::ALL[..index].contains(contract));
    }
}
