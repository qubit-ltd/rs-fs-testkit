// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use qubit_fs::{
    AtomicityRequirement, FileSystemCapabilities, FileSystemCapability, WriteDisposition,
    WriteOptions,
};
use qubit_fs_testkit::FileSystemFixture;

use common::{MemoryFault, MemoryFixture};

/// Verifies the stat contract accepts a conforming provider.
#[test]
fn test_stat_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_stat_contract(&MemoryFixture::new());
}

/// Verifies the read contract accepts a conforming provider.
#[test]
fn test_read_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_read_contract(&MemoryFixture::new());
}

/// Verifies the write contract accepts a conforming provider.
#[test]
fn test_write_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_write_contract(&MemoryFixture::new());
}

/// Verifies the append contract accepts a conforming provider.
#[test]
fn test_append_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_append_contract(&MemoryFixture::new());
}

/// Verifies append rejection preserves the required capability context.
#[test]
fn test_append_contract_accepts_conforming_rejection() {
    let capabilities = FileSystemCapabilities::default()
        .with(FileSystemCapability::Read)
        .with(FileSystemCapability::Write);
    let fixture = MemoryFixture::with_capabilities(capabilities);
    let file_system = fixture.file_system();
    assert!(
        !file_system
            .capabilities()
            .contains(FileSystemCapability::Append)
    );
    let options = WriteOptions {
        disposition: WriteDisposition::Append,
        atomicity: AtomicityRequirement::NotRequired,
        ..WriteOptions::default()
    };
    assert!(
        options
            .validate_against(file_system.capabilities())
            .is_err()
    );

    qubit_fs_testkit::assert_append_contract(&fixture);
}

/// Verifies the atomic-replace contract accepts a conforming provider.
#[test]
fn test_atomic_replace_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_atomic_replace_contract(&MemoryFixture::new());
}

/// Verifies atomic-replace rejection preserves required capability context.
#[test]
fn test_atomic_replace_contract_accepts_conforming_rejection() {
    let capabilities = FileSystemCapabilities::default()
        .with(FileSystemCapability::Read)
        .with(FileSystemCapability::Write);
    let fixture = MemoryFixture::with_capabilities(capabilities);
    let file_system = fixture.file_system();
    assert!(
        !file_system
            .capabilities()
            .contains(FileSystemCapability::AtomicReplace)
    );
    let options = WriteOptions {
        disposition: WriteDisposition::CreateOrReplace,
        atomicity: AtomicityRequirement::Required,
        ..WriteOptions::default()
    };
    assert!(
        options
            .validate_against(file_system.capabilities())
            .is_err()
    );

    qubit_fs_testkit::assert_atomic_replace_contract(&fixture);
}

/// Verifies the list contract accepts a conforming provider.
#[test]
fn test_list_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_list_contract(&MemoryFixture::new());
}

/// Verifies listing accepts providers that cannot load optional metadata.
#[test]
fn test_list_contract_accepts_unknown_metadata() {
    qubit_fs_testkit::assert_list_contract(&MemoryFixture::with_fault(
        MemoryFault::OmitListMetadata,
    ));
}

/// Verifies listing still requires every sibling when metadata is unavailable.
#[test]
#[should_panic(expected = "list must return the written sibling")]
fn test_list_contract_rejects_missing_sibling_without_metadata() {
    qubit_fs_testkit::assert_list_contract(&MemoryFixture::with_fault(
        MemoryFault::OmitListSiblingWithoutMetadata,
    ));
}

/// Verifies the directory-creation contract accepts a conforming provider.
#[test]
fn test_create_dir_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_create_dir_contract(&MemoryFixture::new());
}

/// Verifies the delete contract accepts a conforming provider.
#[test]
fn test_delete_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_delete_contract(&MemoryFixture::new());
}

/// Verifies the rename contract accepts a conforming provider.
#[test]
fn test_rename_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_rename_contract(&MemoryFixture::new());
}

/// Verifies the copy contract accepts a conforming provider.
#[test]
fn test_copy_contract_accepts_conforming_provider() {
    qubit_fs_testkit::assert_copy_contract(&MemoryFixture::new());
}

/// Verifies preflight checks run before provider I/O.
#[test]
fn test_preflight_contract_accepts_conforming_provider() {
    let capabilities = FileSystemCapabilities::default()
        .with(FileSystemCapability::Read)
        .with(FileSystemCapability::Write);
    qubit_fs_testkit::assert_preflight_contract(&MemoryFixture::with_capabilities(capabilities));
}

/// Verifies all synchronous option families reject missing requirements early.
#[test]
fn test_preflight_contract_accepts_all_conforming_rejections() {
    let capabilities = FileSystemCapabilities::default()
        .with(FileSystemCapability::Read)
        .with(FileSystemCapability::Write)
        .with(FileSystemCapability::Delete)
        .with(FileSystemCapability::Rename)
        .with(FileSystemCapability::Copy);

    qubit_fs_testkit::assert_preflight_contract(&MemoryFixture::with_capabilities(capabilities));
}

/// Verifies the stat contract rejects incorrect metadata kinds.
#[test]
#[should_panic(expected = "written resources must have a file-like kind")]
fn test_stat_contract_rejects_wrong_file_kind() {
    qubit_fs_testkit::assert_stat_contract(&MemoryFixture::with_fault(MemoryFault::WrongStatKind));
}

/// Verifies the read contract rejects incorrect opened-file locations.
#[test]
#[should_panic(expected = "reader path must match the requested path")]
fn test_read_contract_rejects_wrong_reader_location() {
    qubit_fs_testkit::assert_read_contract(&MemoryFixture::with_fault(
        MemoryFault::WrongReaderLocation,
    ));
}

/// Verifies the write contract rejects incorrect opened-file locations.
#[test]
#[should_panic(expected = "writer path must match the requested path")]
fn test_write_contract_rejects_wrong_writer_location() {
    qubit_fs_testkit::assert_write_contract(&MemoryFixture::with_fault(
        MemoryFault::WrongWriterLocation,
    ));
}

/// Verifies the append contract rejects replacement behavior.
#[test]
#[should_panic(expected = "committed bytes must match")]
fn test_append_contract_rejects_replacement() {
    qubit_fs_testkit::assert_append_contract(&MemoryFixture::with_fault(
        MemoryFault::AppendReplaces,
    ));
}

/// Verifies the atomic-replace contract rejects downgraded publication.
#[test]
#[should_panic(expected = "required atomic replacement must report atomic publication")]
fn test_atomic_replace_contract_rejects_downgrade() {
    qubit_fs_testkit::assert_atomic_replace_contract(&MemoryFixture::with_fault(
        MemoryFault::AtomicReplaceDowngrade,
    ));
}

/// Verifies the list contract rejects missing entries.
#[test]
#[should_panic(expected = "list must return every written child across pages")]
fn test_list_contract_rejects_missing_entries() {
    qubit_fs_testkit::assert_list_contract(&MemoryFixture::with_fault(MemoryFault::EmptyList));
}

/// Verifies the list contract rejects ignored recursive and prefix options.
#[test]
#[should_panic(expected = "recursive list must return the nested matching child")]
fn test_list_contract_rejects_ignored_options() {
    qubit_fs_testkit::assert_list_contract(&MemoryFixture::with_fault(
        MemoryFault::IgnoreListOptions,
    ));
}

/// Verifies the list contract rejects providers that stop after one requested
/// page.
#[test]
#[should_panic(expected = "list must return every written child across pages")]
fn test_list_contract_rejects_truncated_page_results() {
    qubit_fs_testkit::assert_list_contract(&MemoryFixture::with_fault(
        MemoryFault::TruncateListToPageSize,
    ));
}

/// Verifies the directory-creation contract rejects a no-op implementation.
#[test]
#[should_panic(expected = "nonrecursive directory creation must reject a missing parent")]
fn test_create_dir_contract_rejects_no_op() {
    qubit_fs_testkit::assert_create_dir_contract(&MemoryFixture::with_fault(
        MemoryFault::SkipCreateDir,
    ));
}

/// Verifies directory creation rejects a missing parent without recursion.
#[test]
#[should_panic(expected = "nonrecursive directory creation must reject a missing parent")]
fn test_create_dir_contract_rejects_implicit_parent_creation() {
    qubit_fs_testkit::assert_create_dir_contract(&MemoryFixture::with_fault(
        MemoryFault::CreateDirWithoutParents,
    ));
}

/// Verifies the delete contract rejects a no-op implementation.
#[test]
#[should_panic(expected = "deleted files must not remain present")]
fn test_delete_contract_rejects_no_op() {
    qubit_fs_testkit::assert_delete_contract(&MemoryFixture::with_fault(MemoryFault::SkipDelete));
}

/// Verifies recursive deletion removes descendants as well as the root.
#[test]
#[should_panic(expected = "recursively deleted children must not remain present")]
fn test_delete_contract_rejects_retained_recursive_child() {
    qubit_fs_testkit::assert_delete_contract(&MemoryFixture::with_fault(
        MemoryFault::KeepRecursiveDeleteChild,
    ));
}

/// Verifies the rename contract rejects copy-only behavior.
#[test]
#[should_panic(expected = "rename must remove the source")]
fn test_rename_contract_rejects_source_preservation() {
    qubit_fs_testkit::assert_rename_contract(&MemoryFixture::with_fault(
        MemoryFault::CopyInsteadOfRename,
    ));
}

/// Verifies rename conflicts retain their destination error context.
#[test]
#[should_panic(expected = "filesystem error target must match")]
fn test_rename_contract_rejects_missing_destination_context() {
    qubit_fs_testkit::assert_rename_contract(&MemoryFixture::with_fault(
        MemoryFault::OmitRenameTarget,
    ));
}

/// Verifies advertised atomic rename cannot report a downgraded outcome.
#[test]
#[should_panic(expected = "required atomic rename must report atomic publication")]
fn test_rename_contract_rejects_atomicity_downgrade() {
    qubit_fs_testkit::assert_rename_contract(&MemoryFixture::with_fault(
        MemoryFault::AtomicRenameDowngrade,
    ));
}

/// Verifies the copy contract rejects move behavior.
#[test]
#[should_panic(expected = "committed contract resource must be readable")]
fn test_copy_contract_rejects_source_removal() {
    qubit_fs_testkit::assert_copy_contract(&MemoryFixture::with_fault(
        MemoryFault::MoveInsteadOfCopy,
    ));
}

/// Verifies copy conflicts retain their destination error context.
#[test]
#[should_panic(expected = "filesystem error target must match")]
fn test_copy_contract_rejects_missing_destination_context() {
    qubit_fs_testkit::assert_copy_contract(&MemoryFixture::with_fault(MemoryFault::OmitCopyTarget));
}

/// Verifies the copy contract rejects ignored destination conflict policies.
#[test]
#[should_panic(expected = "copy must reject an existing destination by default")]
fn test_copy_contract_rejects_ignored_conflict_policy() {
    qubit_fs_testkit::assert_copy_contract(&MemoryFixture::with_fault(
        MemoryFault::CopyIgnoresConflict,
    ));
}

/// Verifies the copy contract rejects a no-op tree copy.
#[test]
#[should_panic(expected = "tree copy must report copied files")]
fn test_copy_contract_rejects_skipped_tree_copy() {
    qubit_fs_testkit::assert_copy_contract(&MemoryFixture::with_fault(MemoryFault::SkipTreeCopy));
}

/// Verifies the write contract requires create-new to create a missing target.
#[test]
#[should_panic(expected = "the contract writer must open")]
fn test_write_contract_rejects_missing_create_new_support() {
    qubit_fs_testkit::assert_write_contract(&MemoryFixture::with_fault(
        MemoryFault::RejectCreateNew,
    ));
}

/// Verifies append rejects a missing target instead of creating it.
#[test]
#[should_panic(expected = "append must reject a missing resource")]
fn test_append_contract_rejects_missing_target_creation() {
    qubit_fs_testkit::assert_append_contract(&MemoryFixture::with_fault(
        MemoryFault::AppendCreatesMissing,
    ));
}

/// Verifies the preflight contract rejects provider I/O before validation.
#[test]
#[should_panic(expected = "filesystem error kind must match")]
fn test_preflight_contract_rejects_late_validation() {
    let capabilities = FileSystemCapabilities::default().with(FileSystemCapability::Read);
    let fixture =
        MemoryFixture::with_capabilities_and_fault(capabilities, MemoryFault::SkipReadPreflight);

    qubit_fs_testkit::assert_preflight_contract(&fixture);
}
