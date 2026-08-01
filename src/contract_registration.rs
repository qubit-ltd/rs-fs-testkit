// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Test-harness registration macros for named contract phases.

/// Registers one independently named synchronous test for every contract phase.
///
/// The fixture factory is evaluated separately by each generated test.
#[macro_export]
macro_rules! register_file_system_contract_tests {
    (module: $module:ident, fixture: $fixture:expr $(,)?) => {
        mod $module {
            macro_rules! contract_test {
                ($name:ident, $contract:ident) => {
                    #[cfg_attr(test, test)]
                    fn $name() {
                        let fixture = ($fixture)();
                        $crate::FileSystemContractSuite::new(&fixture)
                            .assert_contract($crate::FileSystemContract::$contract);
                    }
                };
            }

            contract_test!(properties, Properties);
            contract_test!(stat, Stat);
            contract_test!(read, Read);
            contract_test!(write, Write);
            contract_test!(list, List);
            contract_test!(create_directory, CreateDirectory);
            contract_test!(representations, Representations);
            contract_test!(delete, Delete);
            contract_test!(copy, Copy);
            contract_test!(rename, Rename);
            contract_test!(append, Append);
            contract_test!(recursive_delete, RecursiveDelete);
            contract_test!(atomic_rename, AtomicRename);
            contract_test!(atomic_replace, AtomicReplace);
            contract_test!(durable_copy, DurableCopy);
            contract_test!(temp_resources, TempResources);
            contract_test!(error_context, ErrorContext);
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
                            $crate::AsyncFileSystemContractSuite::new(&fixture)
                                .assert_contract($crate::FileSystemContract::$contract)
                                .await;
                        });
                    }
                };
            }

            contract_test!(properties, Properties);
            contract_test!(stat, Stat);
            contract_test!(read, Read);
            contract_test!(write, Write);
            contract_test!(list, List);
            contract_test!(create_directory, CreateDirectory);
            contract_test!(representations, Representations);
            contract_test!(delete, Delete);
            contract_test!(copy, Copy);
            contract_test!(rename, Rename);
            contract_test!(append, Append);
            contract_test!(recursive_delete, RecursiveDelete);
            contract_test!(atomic_rename, AtomicRename);
            contract_test!(atomic_replace, AtomicReplace);
            contract_test!(durable_copy, DurableCopy);
            contract_test!(temp_resources, TempResources);
            contract_test!(error_context, ErrorContext);
        }
    };
}
