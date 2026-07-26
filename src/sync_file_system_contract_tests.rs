// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow inline-tests
//! Defines the complete synchronous contract-suite macro.

/// Generates the complete synchronous filesystem contract suite.
///
/// The fixture expression is evaluated independently for every generated
/// test. It must create a fresh fixture advertising `Read`, `Write`, `List`,
/// `CreateDirectory`, `Delete`, `Rename`, and `Copy`. The suite verifies the
/// advertised `AtomicRename`, `AtomicReplace`, and `RecursiveDelete`
/// guarantees through positive operations. Other optional guarantees are
/// checked through capability dependencies and structured preflight errors.
///
/// The expression is expanded inside the generated module. Provider tests
/// should therefore qualify fixture types or factories through `super::`.
///
/// # Parameters
///
/// * `$module` - Name of the generated test module.
/// * `$fixture` - Expression constructing a fresh [`crate::FileSystemFixture`].
///
/// # Examples
///
/// ```no_run
/// use qubit_fs_testkit::FileSystemFixture;
/// # struct ExampleFixture;
/// # fn fixture() -> ExampleFixture { unimplemented!() }
/// # impl FileSystemFixture for ExampleFixture {
/// #     fn file_system(&self) -> &dyn qubit_fs::FileSystem { unimplemented!() }
/// #     fn path(&self, _: &str) -> qubit_fs::FsPath { unimplemented!() }
/// # }
///
/// qubit_fs_testkit::sync_file_system_contract_tests!(
///     provider_contracts,
///     super::fixture(),
/// );
/// ```
#[macro_export]
macro_rules! sync_file_system_contract_tests {
    ($module:ident, $fixture:expr $(,)?) => {
        mod $module {
            #[test]
            fn test_properties_contract() {
                $crate::assert_properties_contract(&$fixture);
            }

            #[test]
            fn test_capabilities_contract() {
                $crate::assert_capabilities_contract(&$fixture);
            }

            #[test]
            fn test_stat_contract() {
                $crate::assert_stat_contract(&$fixture);
            }

            #[test]
            fn test_read_contract() {
                $crate::assert_read_contract(&$fixture);
            }

            #[test]
            fn test_write_contract() {
                $crate::assert_write_contract(&$fixture);
            }

            #[test]
            fn test_append_contract() {
                $crate::assert_append_contract(&$fixture);
            }

            #[test]
            fn test_atomic_replace_contract() {
                $crate::assert_atomic_replace_contract(&$fixture);
            }

            #[test]
            fn test_list_contract() {
                $crate::assert_list_contract(&$fixture);
            }

            #[test]
            fn test_create_dir_contract() {
                $crate::assert_create_dir_contract(&$fixture);
            }

            #[test]
            fn test_delete_contract() {
                $crate::assert_delete_contract(&$fixture);
            }

            #[test]
            fn test_rename_contract() {
                $crate::assert_rename_contract(&$fixture);
            }

            #[test]
            fn test_copy_contract() {
                $crate::assert_copy_contract(&$fixture);
            }

            #[test]
            fn test_preflight_contract() {
                $crate::assert_preflight_contract(&$fixture);
            }

            #[test]
            fn test_unsupported_operations_contract() {
                $crate::assert_unsupported_operations_contract(&$fixture);
            }
        }
    };
}
