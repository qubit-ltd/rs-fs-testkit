// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Test-harness registration macros for named contract phases.
//!
//! The optional `required_checks` callback can strengthen a generated phase by
//! requiring selected optional checks. It receives the phase once per test and
//! returns check IDs; an ID outside that phase remains an assertion failure.
//!
//! ```rust,ignore
//! qubit_fs_testkit::register_file_system_contract_tests! {
//!     module: provider_contracts,
//!     fixture: super::RootedFixture::new,
//!     required_checks: required_checks,
//! }
//! ```
//!
//! A closure is also accepted:
//!
//! ```rust,ignore
//! qubit_fs_testkit::register_file_system_contract_tests! {
//!     module: provider_contracts,
//!     fixture: super::RootedFixture::new,
//!     required_checks: |phase| match phase {
//!         qubit_fs_testkit::FileSystemContract::Read => vec![
//!             qubit_fs_testkit::ContractCheckId::RandomAccessIdentity,
//!         ],
//!         _ => Vec::new(),
//!     },
//! }
//! ```

/// Registers one independently named synchronous test for every contract phase.
///
/// The fixture factory is evaluated separately by each generated test. Every
/// generated test requires a satisfied run, including cleanup evidence. The
/// optional `required_checks` function or closure is evaluated once for each
/// generated phase before the suite runs. Without it, optional checks remain
/// optional as in previous versions.
#[macro_export]
macro_rules! register_file_system_contract_tests {
    (module: $module:ident, fixture: $fixture:expr, required_checks: $required_checks:expr $(,)?) => {
        $crate::register_file_system_contract_tests!(@register $module, $fixture, $required_checks);
    };
    (module: $module:ident, fixture: $fixture:expr $(,)?) => {
        $crate::register_file_system_contract_tests!(@register $module, $fixture, |_phase: $crate::FileSystemContract| Vec::<$crate::ContractCheckId>::new());
    };
    (@register $module:ident, $fixture:expr, $required_checks:expr) => {
        mod $module {
            macro_rules! contract_test {
                ($name:ident, $contract:ident) => {
                    #[cfg_attr(test, test)]
                    fn $name() {
                        let phase = $crate::FileSystemContract::$contract;
                        let required: Vec<$crate::ContractCheckId> = ($required_checks)(phase);
                        let fixture = ($fixture)();
                        let mut suite = $crate::FileSystemContractSuite::new(&fixture);
                        suite.run_contract(phase).assert_satisfied_with(&required);
                    }
                };
            }

            contract_test!(test_properties, Properties);
            contract_test!(test_stat, Stat);
            contract_test!(test_read, Read);
            contract_test!(test_write, Write);
            contract_test!(test_list, List);
            contract_test!(test_create_directory, CreateDirectory);
            contract_test!(test_representations, Representations);
            contract_test!(test_delete, Delete);
            contract_test!(test_copy, Copy);
            contract_test!(test_rename, Rename);
            contract_test!(test_temp_resources, TempResources);
            contract_test!(test_error_context, ErrorContext);
        }
    };
}

/// Registers one independently named asynchronous test for every contract
/// phase.
///
/// `runner` must drive the supplied future to completion. `required_checks`,
/// when present, is a function or closure evaluated once per phase before the
/// runner starts. It selects optional checks that must have evidence; IDs
/// outside the selected phase fail the normal contract assertion.
///
/// The runner receives the whole future that owns the fixture and suite. A
/// runner must poll that future until all contract assertions finish; returning
/// early, including after catching an assertion panic, fails the generated
/// test with a `[fs-testkit:runner/incomplete]` diagnostic. This completion
/// check does not impose a deadline or recover cleanup after the future is
/// dropped. A timeout around that future can end a test, but it drops the
/// wrapper and gives the caller no retained suite with which to inspect an
/// interrupted run or retry `finish()`. For bounded remote-provider checks that
/// need retained diagnostics and cleanup control, use a manually managed suite
/// with separate run and cleanup budgets as shown in the [user
/// guide](https://github.com/qubit-ltd/rs-fs-testkit/blob/main/doc/user_guide.md#bounded-asynchronous-runs-and-cleanup).
#[macro_export]
macro_rules! register_async_file_system_contract_tests {
    (
        module: $module:ident,
        fixture: $fixture:expr,
        runner: $runner:expr,
        required_checks: $required_checks:expr $(,)?
    ) => {
        $crate::register_async_file_system_contract_tests!(@register $module, $fixture, $runner, $required_checks);
    };
    (
        module: $module:ident,
        fixture: $fixture:expr,
        runner: $runner:expr $(,)?
    ) => {
        $crate::register_async_file_system_contract_tests!(@register $module, $fixture, $runner, |_phase: $crate::FileSystemContract| Vec::<$crate::ContractCheckId>::new());
    };
    (@register $module:ident, $fixture:expr, $runner:expr, $required_checks:expr) => {
        mod $module {
            macro_rules! contract_test {
                ($name:ident, $contract:ident) => {
                    #[cfg_attr(test, test)]
                    fn $name() {
                        let phase = $crate::FileSystemContract::$contract;
                        let required: Vec<$crate::ContractCheckId> = ($required_checks)(phase);
                        let fixture = ($fixture)();
                        let completed = ::std::sync::Arc::new(
                            ::std::sync::atomic::AtomicBool::new(false),
                        );
                        let completion = ::std::sync::Arc::clone(&completed);
                        ($runner)(async move {
                            {
                                let mut suite = $crate::AsyncFileSystemContractSuite::new(&fixture);
                                suite.run_contract(phase).await.assert_satisfied_with(&required);
                            }
                            completion.store(true, ::std::sync::atomic::Ordering::Release);
                        });
                        assert!(
                            completed.load(::std::sync::atomic::Ordering::Acquire),
                            "[fs-testkit:runner/incomplete] phase {:?}: runner returned before contract assertions completed",
                            phase
                        );
                    }
                };
            }

            contract_test!(test_properties, Properties);
            contract_test!(test_stat, Stat);
            contract_test!(test_read, Read);
            contract_test!(test_write, Write);
            contract_test!(test_list, List);
            contract_test!(test_create_directory, CreateDirectory);
            contract_test!(test_representations, Representations);
            contract_test!(test_delete, Delete);
            contract_test!(test_copy, Copy);
            contract_test!(test_rename, Rename);
            contract_test!(test_temp_resources, TempResources);
            contract_test!(test_error_context, ErrorContext);
        }
    };
}
