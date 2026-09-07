// qubit-style: allow explicit-imports
// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================

mod common;

#[cfg(feature = "async")]
use qubit_fs_testkit::register_async_file_system_contract_tests;
use qubit_fs_testkit::register_file_system_contract_tests;

#[cfg(feature = "async")]
use self::common::AsyncMemoryFixture;
use self::common::MemoryFixture;
#[cfg(feature = "async")]
use self::common::async_memory_file_system::run_controlled;

register_file_system_contract_tests! {
    module: registered_sync_contracts,
    fixture: super::registration_fixture,
}

#[cfg(feature = "async")]
register_async_file_system_contract_tests! {
    module: registered_async_contracts,
    fixture: super::AsyncMemoryFixture::new,
    runner: super::run_controlled,
}

/// Uses a missing-evidence fixture only in the isolated harness child.
fn registration_fixture() -> MemoryFixture {
    if std::env::var_os("FS_TESTKIT_REGISTRATION_MISSING_EVIDENCE").is_some() {
        MemoryFixture::with_conditional_case_unavailable(crate::common::UnavailableScenario::ReadIfMatch)
    } else {
        MemoryFixture::new()
    }
}

/// Default macro registration must reject missing evidence without a strict
/// flag.
#[test]
fn default_registration_rejects_missing_evidence() {
    let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", "registered_sync_contracts::read", "--nocapture"])
        .env("FS_TESTKIT_REGISTRATION_MISSING_EVIDENCE", "1")
        .output()
        .expect("run isolated contract registration");
    assert!(
        !output.status.success(),
        "default registration accepted missing read evidence"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("read/if-match"),
        "failure must identify missing conditional evidence: {stderr}"
    );
}
