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
    module: native_copy_missing_sync,
    fixture: super::missing_native_copy_sync_fixture,
    required_checks: |phase| match phase {
        qubit_fs_testkit::FileSystemContract::Copy
            if std::env::var_os("FS_TESTKIT_REGISTRATION_NATIVE_COPY_REQUIRED").is_some() => vec![
            qubit_fs_testkit::ContractCheckId::CopyNativeNonEmpty,
        ],
        _ => Vec::new(),
    },
}

fn missing_native_copy_sync_fixture() -> common::native_copy_fixture::NativeCopyFixture {
    let fixture = common::native_copy_fixture::NativeCopyFixture::new(false, false);
    fixture.inject_native_fault("prepare_unsupported");
    fixture
}

register_file_system_contract_tests! {
    module: native_copy_supported_sync,
    fixture: || super::common::native_copy_fixture::NativeCopyFixture::new(false, false),
    required_checks: |phase| match phase {
        qubit_fs_testkit::FileSystemContract::Copy => vec![
            qubit_fs_testkit::ContractCheckId::CopyNativeNonEmpty,
        ],
        _ => Vec::new(),
    },
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
    module: native_copy_missing_async,
    fixture: super::missing_native_copy_async_fixture,
    runner: super::common::async_memory_file_system::run_controlled,
    required_checks: |phase| match phase {
        qubit_fs_testkit::FileSystemContract::Copy
            if std::env::var_os("FS_TESTKIT_REGISTRATION_NATIVE_COPY_REQUIRED").is_some() => vec![
            qubit_fs_testkit::ContractCheckId::CopyNativeNonEmpty,
        ],
        _ => Vec::new(),
    },
}

#[cfg(feature = "async")]
fn missing_native_copy_async_fixture() -> common::native_copy_fixture::AsyncNativeCopyFixture {
    let fixture = common::native_copy_fixture::AsyncNativeCopyFixture::new(false, false);
    fixture.inject_native_fault("prepare_unsupported");
    fixture
}

#[cfg(feature = "async")]
register_async_file_system_contract_tests! {
    module: native_copy_supported_async,
    fixture: || super::common::native_copy_fixture::AsyncNativeCopyFixture::new(false, false),
    runner: super::common::async_memory_file_system::run_controlled,
    required_checks: |phase| match phase {
        qubit_fs_testkit::FileSystemContract::Copy => vec![
            qubit_fs_testkit::ContractCheckId::CopyNativeNonEmpty,
        ],
        _ => Vec::new(),
    },
}

#[cfg(feature = "async")]
register_async_file_system_contract_tests! {
    module: strict_async,
    fixture: super::common::AsyncMemoryFixture::new,
    runner: super::common::async_memory_file_system::run_controlled,
    required_checks: super::required_random_access,
}

#[cfg(feature = "async")]
register_async_file_system_contract_tests! {
    module: incomplete_async,
    fixture: super::incomplete_async_fixture,
    runner: super::async_registration_runner,
    required_checks: super::required_async_evidence,
}

#[cfg(feature = "async")]
fn incomplete_async_fixture() -> common::AsyncMemoryFixture {
    match std::env::var_os("FS_TESTKIT_REGISTRATION_ASYNC_MODE").as_deref() {
        Some(mode) if mode == "catch-assertion" => {
            common::AsyncMemoryFixture::with_conditional_case_unavailable(common::UnavailableScenario::ReadIfMatch)
        }
        Some(mode) if mode == "pending-drop" => {
            common::AsyncMemoryFixture::with_cancellation_limits(qubit_fs::metadata::FileSystemLimits::unknown())
        }
        _ => common::AsyncMemoryFixture::new(),
    }
}

#[cfg(feature = "async")]
fn required_async_evidence(phase: qubit_fs_testkit::FileSystemContract) -> Vec<qubit_fs_testkit::ContractCheckId> {
    if phase == qubit_fs_testkit::FileSystemContract::Read
        && std::env::var_os("FS_TESTKIT_REGISTRATION_ASYNC_MODE").as_deref()
            == Some(std::ffi::OsStr::new("catch-assertion"))
    {
        vec![qubit_fs_testkit::ContractCheckId::ReadIfMatchCurrent]
    } else {
        Vec::new()
    }
}

#[cfg(feature = "async")]
fn async_registration_runner(future: impl std::future::Future<Output = ()>) {
    match std::env::var_os("FS_TESTKIT_REGISTRATION_ASYNC_MODE").as_deref() {
        Some(mode) if mode == "drop-before-poll" => drop(future),
        Some(mode) if mode == "pending-drop" => {
            use std::future::Future;
            use std::task::Context;
            use std::task::Poll;
            use std::task::Waker;

            let mut contract = Box::pin(future);
            let mut first_poll = true;
            let mut future = Box::pin(std::future::poll_fn(|context| {
                if first_poll {
                    first_poll = false;
                    assert!(
                        contract.as_mut().poll(context).is_pending(),
                        "contract future must reach Pending before the runner drops it"
                    );
                }
                Poll::<()>::Pending
            }));
            let mut context = Context::from_waker(Waker::noop());
            assert!(future.as_mut().poll(&mut context).is_pending());
            drop(future);
        }
        Some(mode) if mode == "catch-assertion" => {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                common::async_memory_file_system::run_controlled(future)
            }));
        }
        _ => common::async_memory_file_system::run_controlled(future),
    }
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
    run_child_with_env(test_name, mode, "1")
}

fn run_child_with_env(test_name: &str, key: Option<&str>, value: &str) -> (bool, String, String) {
    let mut command = std::process::Command::new(std::env::current_exe().expect("test executable"));
    command.args(["--exact", test_name, "--nocapture"]);
    if let Some(key) = key {
        command.env(key, value);
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
fn test_registration_can_require_native_copy_only_for_copy_phase() {
    let (missing_success, _, missing_stderr) = run_child(
        "native_copy_missing_sync::test_copy",
        Some("FS_TESTKIT_REGISTRATION_NATIVE_COPY_REQUIRED"),
    );
    assert!(!missing_success, "required native-copy evidence was missing");
    assert!(missing_stderr.contains("copy/native-nonempty"), "{missing_stderr}");
    let (supported_success, _, supported_stderr) = run_child("native_copy_supported_sync::test_copy", None);
    assert!(
        supported_success,
        "supported native-copy evidence was rejected: {supported_stderr}"
    );
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

#[cfg(feature = "async")]
#[test]
fn test_async_registration_can_require_native_copy_only_for_copy_phase() {
    let (missing_success, _, missing_stderr) = run_child(
        "native_copy_missing_async::test_copy",
        Some("FS_TESTKIT_REGISTRATION_NATIVE_COPY_REQUIRED"),
    );
    assert!(!missing_success, "required async native-copy evidence was missing");
    assert!(missing_stderr.contains("copy/native-nonempty"), "{missing_stderr}");
    let (supported_success, _, supported_stderr) = run_child("native_copy_supported_async::test_copy", None);
    assert!(
        supported_success,
        "supported async native-copy evidence was rejected: {supported_stderr}"
    );
}

#[cfg(feature = "async")]
#[test]
fn test_async_runner_must_complete_before_returning() {
    for mode in ["drop-before-poll", "pending-drop"] {
        let (success, stdout, stderr) = run_child_with_env(
            "incomplete_async::test_write",
            Some("FS_TESTKIT_REGISTRATION_ASYNC_MODE"),
            mode,
        );
        assert!(stdout.contains("running 1 test"), "{mode}: {stdout}");
        assert!(!success, "{mode} unexpectedly passed");
        assert!(stderr.contains("[fs-testkit:runner/incomplete]"), "{mode}: {stderr}");
    }
}

#[cfg(feature = "async")]
#[test]
fn test_async_runner_cannot_swallow_contract_assertion() {
    let (success, stdout, stderr) = run_child_with_env(
        "incomplete_async::test_read",
        Some("FS_TESTKIT_REGISTRATION_ASYNC_MODE"),
        "catch-assertion",
    );
    assert!(stdout.contains("running 1 test"), "{stdout}");
    assert!(!success, "runner swallowed a contract assertion");
    assert!(stderr.contains("[fs-testkit:runner/incomplete]"), "{stderr}");
    assert!(stderr.contains("read/if-match-current"), "{stderr}");
}
