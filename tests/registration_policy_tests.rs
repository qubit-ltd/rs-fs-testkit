// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

pub use ::qubit_fs_testkit;
#[cfg(feature = "async")]
use qubit_fs_testkit::register_async_file_system_contract_tests;
use qubit_fs_testkit::register_file_system_contract_tests;

mod common;

use self::common::MemoryFixture;

register_file_system_contract_tests! {
    module: default_sync,
    fixture: super::MemoryFixture::new,
}

register_file_system_contract_tests! {
    module: strict_sync_function,
    fixture: super::MemoryFixture::new,
    required_checks: super::required_random_access,
}

register_file_system_contract_tests! {
    module: strict_sync_closure,
    fixture: super::MemoryFixture::new,
    required_checks: |phase| match phase {
        qubit_fs_testkit::FileSystemContract::Read
            if std::env::var_os("FS_TESTKIT_REGISTRATION_STRICT").is_some() => vec![
            qubit_fs_testkit::ContractCheckId::RandomAccessIdentity,
        ],
        _ => Vec::new(),
    },
}

register_file_system_contract_tests! {
    module: wrong_phase_sync,
    fixture: super::MemoryFixture::new,
    required_checks: |phase| match phase {
        qubit_fs_testkit::FileSystemContract::Properties
            if std::env::var_os("FS_TESTKIT_REGISTRATION_WRONG_PHASE").is_some() => vec![
            qubit_fs_testkit::ContractCheckId::RandomAccessIdentity,
        ],
        _ => Vec::new(),
    },
}

register_file_system_contract_tests! {
    module: not_applicable_sync,
    fixture: super::MemoryFixture::without_delete,
    required_checks: |phase| match phase {
        qubit_fs_testkit::FileSystemContract::Delete
            if std::env::var_os("FS_TESTKIT_REGISTRATION_NOT_APPLICABLE").is_some() => vec![
            qubit_fs_testkit::ContractCheckId::DeleteIfMatch,
        ],
        _ => Vec::new(),
    },
}

#[cfg(feature = "async")]
register_async_file_system_contract_tests! {
    module: default_async,
    fixture: super::common::AsyncMemoryFixture::new,
    runner: super::common::async_memory_file_system::run_controlled,
}

#[cfg(feature = "async")]
register_async_file_system_contract_tests! {
    module: strict_async,
    fixture: super::common::AsyncMemoryFixture::new,
    runner: super::common::async_memory_file_system::run_controlled,
    required_checks: super::required_random_access,
}

fn required_random_access(phase: qubit_fs_testkit::FileSystemContract) -> Vec<qubit_fs_testkit::ContractCheckId> {
    match phase {
        qubit_fs_testkit::FileSystemContract::Read if std::env::var_os("FS_TESTKIT_REGISTRATION_STRICT").is_some() => {
            vec![qubit_fs_testkit::ContractCheckId::RandomAccessIdentity]
        }
        _ => Vec::new(),
    }
}

fn run_child(test_name: &str, mode: Option<&str>) -> (bool, String, String) {
    let mut command = std::process::Command::new(std::env::current_exe().expect("test executable"));
    command.args(["--exact", test_name, "--nocapture"]);
    if let Some(mode) = mode {
        command.env(mode, "1");
    }
    let output = command.output().expect("run isolated registration test");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn test_default_registration_keeps_optional_identity_optional() {
    let (success, stdout, stderr) = run_child("default_sync::test_read", None);
    assert!(stdout.contains("running 1 test"), "{stdout}");
    assert!(success, "default policy rejected the optional identity check: {stderr}");
}

#[test]
fn test_function_and_closure_policies_reject_missing_identity_evidence() {
    for test_name in ["strict_sync_function::test_read", "strict_sync_closure::test_read"] {
        let (success, stdout, stderr) = run_child(test_name, Some("FS_TESTKIT_REGISTRATION_STRICT"));
        assert!(stdout.contains("running 1 test"), "{test_name}: {stdout}");
        assert!(!success, "strict policy accepted missing identity evidence");
        assert!(
            stderr.contains("read/random-access-identity"),
            "failure should identify the required check: {stderr}"
        );
    }
}

#[test]
fn test_check_from_another_phase_fails_instead_of_being_filtered() {
    let (success, stdout, stderr) = run_child(
        "wrong_phase_sync::test_properties",
        Some("FS_TESTKIT_REGISTRATION_WRONG_PHASE"),
    );
    assert!(stdout.contains("running 1 test"), "{stdout}");
    assert!(!success, "phase-external check ID was silently ignored");
    assert!(stderr.contains("outside this run"), "{stderr}");
}

#[test]
fn test_not_applicable_required_check_remains_satisfied() {
    let (success, stdout, stderr) = run_child(
        "not_applicable_sync::test_delete",
        Some("FS_TESTKIT_REGISTRATION_NOT_APPLICABLE"),
    );
    assert!(stdout.contains("running 1 test"), "{stdout}");
    assert!(
        success,
        "NotApplicable evidence should satisfy a required check: {stderr}"
    );
}

#[cfg(feature = "async")]
#[test]
fn test_async_default_and_strict_registration_apply_the_same_policy() {
    let (default_success, stdout, stderr) = run_child("default_async::test_read", None);
    assert!(stdout.contains("running 1 test"), "{stdout}");
    assert!(default_success, "async default policy changed: {stderr}");

    let (strict_success, stdout, stderr) = run_child("strict_async::test_read", Some("FS_TESTKIT_REGISTRATION_STRICT"));
    assert!(stdout.contains("running 1 test"), "{stdout}");
    assert!(!strict_success, "async strict policy accepted missing evidence");
    assert!(stderr.contains("read/random-access-identity"), "{stderr}");
}
