// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Test-harness registration macros for named contract phases.

/// Registers one independently named synchronous test for every contract phase.
///
/// The fixture factory is evaluated separately by each generated test.
/// Every generated test requires a satisfied run, including cleanup evidence.
#[macro_export]
macro_rules! register_file_system_contract_tests {
    (module: $module:ident, fixture: $fixture:expr $(,)?) => {
        mod $module {
            macro_rules! contract_test {
                ($name:ident, $contract:ident) => {
                    #[cfg_attr(test, test)]
                    fn $name() {
                        let fixture = ($fixture)();
                        let mut suite = $crate::FileSystemContractSuite::new(&fixture);
                        suite
                            .run_contract($crate::FileSystemContract::$contract)
                            .assert_satisfied();
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
/// `runner` must be a function or closure that drives the supplied future to
/// completion using the provider crate's asynchronous runtime.
#[macro_export]
macro_rules! register_async_file_system_contract_tests {
    (
        module: $module:ident,
        fixture: $fixture:expr,
        runner: $runner:expr $(,)?
    ) => {
        mod $module {
            macro_rules! contract_test {
                ($name:ident, $contract:ident) => {
                    #[cfg_attr(test, test)]
                    fn $name() {
                        let fixture = ($fixture)();
                        ($runner)(async move {
                            let mut suite = $crate::AsyncFileSystemContractSuite::new(&fixture);
                            suite
                                .run_contract($crate::FileSystemContract::$contract)
                                .await
                                .assert_satisfied();
                        });
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
