// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Contract assertions for synchronous filesystem I/O.

use qubit_fs::{
    AchievedAtomicity,
    AtomicityRequirement,
    ChecksumPolicy,
    CopyConflictPolicy,
    CopyOptions,
    CreateDirOptions,
    DeleteOptions,
    DirectoryStreamExt,
    FileKind,
    FileSystem,
    FileSystemCapability,
    FileSystemExt,
    FsErrorKind,
    FsOperation,
    FsPath,
    ListOptions,
    PathSemantics,
    ReadOptions,
    RenameOptions,
    ResourceVersion,
    ServerSidePreference,
    WriteDisposition,
    WriteOptions,
    WriteOutcome,
    WritePrecondition,
};
use qubit_io::Output;

use crate::{
    FileSystemFixture,
    internal::{
        assert_error,
        assert_error_with_target,
    },
};

const INITIAL_CONTENT: &[u8] = b"initial contract content";
const REPLACEMENT_CONTENT: &[u8] = b"replacement contract content";
const APPENDED_CONTENT: &[u8] = b" + appended";

/// Checks metadata and existence behavior for missing and written files.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture supporting write operations.
///
/// # Panics
///
/// Panics when missing-path errors lose context, existence disagrees with
/// metadata, or written-file metadata is inconsistent.
#[track_caller]
pub fn assert_stat_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    require_capability(file_system, FileSystemCapability::Write);
    let missing = fixture.path("contract-stat-missing.bin");
    let provider = file_system.info().provider_id();

    let error = file_system
        .stat(&missing)
        .expect_err("stat must report a missing fixture path");
    assert_error(
        &error,
        FsErrorKind::NotFound,
        FsOperation::Stat,
        Some(&missing),
        Some(provider),
        None,
    );
    assert!(
        !file_system
            .exists(&missing)
            .expect("exists must accept a confirmed missing path"),
        "exists must return false for a confirmed missing path",
    );

    let written = fixture.path("contract-stat-written.bin");
    write_bytes(
        file_system,
        &written,
        WriteOptions::default(),
        INITIAL_CONTENT,
    );
    let metadata = file_system
        .stat(&written)
        .expect("stat must read metadata for a written file");
    assert_file_like_kind(
        file_system.info().path_semantics(),
        &metadata.kind,
        "written resources must have a file-like kind",
    );
    if let Some(length) = metadata.len {
        assert_eq!(
            INITIAL_CONTENT.len() as u64,
            length,
            "known metadata length must match written bytes",
        );
    }
    assert!(
        file_system
            .exists(&written)
            .expect("exists must inspect a written file"),
        "exists must return true for a written file",
    );
}

/// Checks complete reads, reader identity, and caller byte limits.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture supporting read and write operations.
///
/// # Panics
///
/// Panics when bytes do not round-trip, opened-file identity is wrong, or
/// caller byte limits do not produce the required structured error.
#[track_caller]
pub fn assert_read_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    let path = fixture.path("contract-read.bin");
    write_bytes(file_system, &path, WriteOptions::default(), INITIAL_CONTENT);

    let bytes = file_system
        .read_all(&path, INITIAL_CONTENT.len())
        .expect("read_all must read the complete contract resource");
    assert_eq!(
        INITIAL_CONTENT,
        bytes.as_slice(),
        "read bytes must match committed bytes",
    );

    let reader = file_system
        .open_reader(&path, ReadOptions::default())
        .expect("open_reader must open the contract resource");
    assert_eq!(
        file_system.info().id(),
        reader.info().location().file_system_id(),
        "reader filesystem ID must match the configured filesystem",
    );
    assert_eq!(
        &path,
        reader.info().location().path(),
        "reader path must match the requested path",
    );

    let error = file_system
        .read_all(&path, INITIAL_CONTENT.len() - 1)
        .expect_err("read_all must enforce the caller byte limit");
    assert_error(
        &error,
        FsErrorKind::ResourceLimitExceeded,
        FsOperation::Read,
        Some(&path),
        None,
        None,
    );
}

/// Checks create, replace, writer identity, and create-new behavior.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture supporting read and write operations.
///
/// # Panics
///
/// Panics when committed bytes, write outcomes, opened-file identity, or
/// create-new preservation violate the common contract.
#[track_caller]
pub fn assert_write_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    let path = fixture.path("contract-write.bin");

    let mut writer = file_system
        .open_writer(&path, WriteOptions::default())
        .expect("open_writer must create the contract resource");
    assert_eq!(
        file_system.info().id(),
        writer.info().location().file_system_id(),
        "writer filesystem ID must match the configured filesystem",
    );
    assert_eq!(
        &path,
        writer.info().location().path(),
        "writer path must match the requested path",
    );
    writer
        .write_fully(INITIAL_CONTENT)
        .expect("writer must accept the complete contract payload");
    let outcome = writer.commit().expect("writer must commit the payload");
    assert_bytes_written(&outcome, INITIAL_CONTENT.len());
    assert_content(file_system, &path, INITIAL_CONTENT);

    let outcome = write_bytes(
        file_system,
        &path,
        WriteOptions::default(),
        REPLACEMENT_CONTENT,
    );
    assert_bytes_written(&outcome, REPLACEMENT_CONTENT.len());
    assert_content(file_system, &path, REPLACEMENT_CONTENT);

    let options = WriteOptions {
        disposition: WriteDisposition::CreateNew,
        atomicity: AtomicityRequirement::NotRequired,
        ..WriteOptions::default()
    };
    let create_new_path = fixture.path("contract-write-create-new.bin");
    let outcome = write_bytes(
        file_system,
        &create_new_path,
        options.clone(),
        INITIAL_CONTENT,
    );
    assert_bytes_written(&outcome, INITIAL_CONTENT.len());
    assert_content(file_system, &create_new_path, INITIAL_CONTENT);
    let error = file_system
        .open_writer(&path, options)
        .expect_err("create-new must reject an existing resource");
    assert_error(
        &error,
        FsErrorKind::AlreadyExists,
        FsOperation::OpenWriter,
        Some(&path),
        Some(file_system.info().provider_id()),
        None,
    );
    assert_content(file_system, &path, REPLACEMENT_CONTENT);
}

/// Checks append behavior or its capability-gated rejection.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture supporting read and write operations.
///
/// # Panics
///
/// Panics when append does not preserve existing bytes or an unsupported
/// append request loses its structured capability context.
#[track_caller]
pub fn assert_append_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    let path = fixture.path("contract-append.bin");
    write_bytes(file_system, &path, WriteOptions::default(), INITIAL_CONTENT);
    let options = WriteOptions {
        disposition: WriteDisposition::Append,
        atomicity: AtomicityRequirement::NotRequired,
        ..WriteOptions::default()
    };

    if file_system
        .capabilities()
        .contains(FileSystemCapability::Append)
    {
        write_bytes(file_system, &path, options, APPENDED_CONTENT);
        let mut expected = INITIAL_CONTENT.to_vec();
        expected.extend_from_slice(APPENDED_CONTENT);
        assert_content(file_system, &path, &expected);
        let missing = fixture.path("contract-append-missing.bin");
        let error = file_system
            .open_writer(
                &missing,
                WriteOptions {
                    disposition: WriteDisposition::Append,
                    atomicity: AtomicityRequirement::NotRequired,
                    ..WriteOptions::default()
                },
            )
            .expect_err("append must reject a missing resource");
        assert_error(
            &error,
            FsErrorKind::NotFound,
            FsOperation::OpenWriter,
            Some(&missing),
            Some(file_system.info().provider_id()),
            None,
        );
    } else {
        let error = file_system
            .open_writer(&path, options)
            .expect_err("append must be rejected when it is not advertised");
        assert_error(
            &error,
            FsErrorKind::RequirementNotMet,
            FsOperation::OpenWriter,
            Some(&path),
            Some(file_system.info().provider_id()),
            Some(FileSystemCapability::Append),
        );
        assert_content(file_system, &path, INITIAL_CONTENT);
    }
}

/// Checks required atomic replacement or its capability-gated rejection.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture supporting read and write operations.
///
/// # Panics
///
/// Panics when an advertised atomic replacement is not achieved or a rejected
/// request loses its structured capability context or modifies existing data.
#[track_caller]
pub fn assert_atomic_replace_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    let path = fixture.path("contract-atomic-replace.bin");
    write_bytes(file_system, &path, WriteOptions::default(), INITIAL_CONTENT);
    let options = WriteOptions {
        disposition: WriteDisposition::CreateOrReplace,
        atomicity: AtomicityRequirement::Required,
        ..WriteOptions::default()
    };

    if file_system
        .capabilities()
        .contains(FileSystemCapability::AtomicReplace)
    {
        let outcome =
            write_bytes(file_system, &path, options, REPLACEMENT_CONTENT);
        assert_eq!(
            AchievedAtomicity::Atomic,
            outcome.atomicity,
            "required atomic replacement must report atomic publication",
        );
        assert_content(file_system, &path, REPLACEMENT_CONTENT);
    } else {
        let error = file_system
            .open_writer(&path, options)
            .expect_err("required atomic replacement must be rejected");
        assert_error(
            &error,
            FsErrorKind::RequirementNotMet,
            FsOperation::OpenWriter,
            Some(&path),
            Some(file_system.info().provider_id()),
            Some(FileSystemCapability::AtomicReplace),
        );
        assert_content(file_system, &path, INITIAL_CONTENT);
    }
}

/// Checks non-recursive listing, recursive prefix filtering, and entry
/// metadata.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture supporting list and write operations.
///
/// # Panics
///
/// Panics when written children are missing from their expected listings, when
/// recursive prefix filtering is ignored, or when entry identity, kind, or
/// provider-reported metadata is inconsistent.
#[track_caller]
pub fn assert_list_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    require_capability(file_system, FileSystemCapability::List);
    require_capability(file_system, FileSystemCapability::Write);
    let directory = fixture.path("contract-list");
    let child = fixture.path("contract-list/child.bin");
    let sibling = fixture.path("contract-list/sibling.bin");
    write_bytes(
        file_system,
        &child,
        WriteOptions {
            create_parent: true,
            ..WriteOptions::default()
        },
        INITIAL_CONTENT,
    );
    write_bytes(
        file_system,
        &sibling,
        WriteOptions {
            create_parent: true,
            ..WriteOptions::default()
        },
        REPLACEMENT_CONTENT,
    );

    let entries = file_system
        .list(
            &directory,
            ListOptions {
                include_metadata: true,
                page_size: Some(1),
                ..ListOptions::default()
            },
        )
        .expect("list must open the contract directory")
        .collect_entries(16)
        .expect("list must enumerate the contract directory");
    assert_eq!(
        2,
        entries.len(),
        "list must return every written child across pages",
    );
    let entry = entries
        .iter()
        .find(|entry| entry.path == child)
        .expect("list must return the written child");
    assert_eq!(&child, &entry.path, "listed path must match the child");
    assert_eq!(
        child
            .file_name()
            .expect("contract child paths must have a basename"),
        entry.name,
        "listed name must match the mapped child basename",
    );
    assert_file_like_kind(
        file_system.info().path_semantics(),
        &entry.kind,
        "listed child must have a file-like kind",
    );
    assert!(
        entries.iter().any(|entry| entry.path == sibling),
        "list must return the written sibling",
    );
    if let Some(metadata) = &entry.metadata {
        assert_eq!(
            &entry.kind, &metadata.kind,
            "listed metadata kind must match the entry"
        );
        if let Some(length) = metadata.len {
            assert_eq!(
                INITIAL_CONTENT.len() as u64,
                length,
                "known listed metadata length must match the written bytes",
            );
        }
    }

    let nested_child = fixture.path("contract-list/nested/match.bin");
    write_bytes(
        file_system,
        &nested_child,
        WriteOptions {
            create_parent: true,
            ..WriteOptions::default()
        },
        REPLACEMENT_CONTENT,
    );
    let entries = file_system
        .list(
            &directory,
            ListOptions {
                recursive: true,
                prefix: Some(
                    fixture.list_prefix(&directory, "nested/match.bin"),
                ),
                ..ListOptions::default()
            },
        )
        .expect("recursive list must open the contract directory")
        .collect_entries(16)
        .expect("recursive list must enumerate the contract directory");
    assert_eq!(
        1,
        entries.len(),
        "recursive list must return the nested matching child",
    );
    assert_eq!(
        &nested_child, &entries[0].path,
        "recursive prefix filtering must retain the matching child",
    );
}

/// Checks nonrecursive failures, recursive directory creation, and idempotent
/// existing-directory use.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture supporting directory creation.
///
/// # Panics
///
/// Panics when nonrecursive creation accepts missing parents, recursive
/// creation does not create a directory, or existing-directory policies are
/// ignored.
#[track_caller]
pub fn assert_create_dir_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    require_capability(file_system, FileSystemCapability::CreateDirectory);
    if file_system.info().path_semantics() == PathSemantics::Hierarchical {
        let missing_parent = fixture.path("contract-create-dir/missing/child");
        let error = file_system
            .create_dir(&missing_parent, CreateDirOptions::default())
            .expect_err(
                "nonrecursive directory creation must reject a missing parent",
            );
        assert_error(
            &error,
            FsErrorKind::NotFound,
            FsOperation::CreateDir,
            Some(&missing_parent),
            Some(file_system.info().provider_id()),
            None,
        );
    }
    let directory = fixture.path("contract-create-dir/child");
    file_system
        .create_dir(
            &directory,
            CreateDirOptions {
                recursive: true,
                ..CreateDirOptions::default()
            },
        )
        .expect("recursive directory creation must succeed");
    let metadata = file_system
        .stat(&directory)
        .expect("created directory metadata must be readable");
    assert!(
        metadata.is_directory_like(),
        "created resource must be a directory-like container",
    );
    file_system
        .create_dir(
            &directory,
            CreateDirOptions {
                exists_ok: true,
                ..CreateDirOptions::default()
            },
        )
        .expect("exists_ok must accept an existing directory");
    let error = file_system
        .create_dir(&directory, CreateDirOptions::default())
        .expect_err("existing directories must reject exists_ok=false");
    assert_error(
        &error,
        FsErrorKind::AlreadyExists,
        FsOperation::CreateDir,
        Some(&directory),
        Some(file_system.info().provider_id()),
        None,
    );
}

/// Checks file deletion, missing-target tolerance, and recursive deletion when
/// advertised.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture supporting write and delete operations.
///
/// # Panics
///
/// Panics when deletion leaves a resource present, `missing_ok` fails, or an
/// advertised recursive deletion leaves its tree present.
#[track_caller]
pub fn assert_delete_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    require_capability(file_system, FileSystemCapability::Write);
    require_capability(file_system, FileSystemCapability::Delete);
    let path = fixture.path("contract-delete.bin");
    write_bytes(file_system, &path, WriteOptions::default(), INITIAL_CONTENT);
    file_system
        .delete(&path, DeleteOptions::default())
        .expect("delete must remove the contract file");
    assert!(
        !file_system
            .exists(&path)
            .expect("exists must inspect the deleted file"),
        "deleted files must not remain present",
    );
    file_system
        .delete(
            &path,
            DeleteOptions {
                missing_ok: true,
                ..DeleteOptions::default()
            },
        )
        .expect("missing_ok must accept a missing target");

    if file_system
        .capabilities()
        .contains(FileSystemCapability::RecursiveDelete)
    {
        let child = fixture.path("contract-delete-tree/child.bin");
        write_bytes(
            file_system,
            &child,
            WriteOptions {
                create_parent: true,
                ..WriteOptions::default()
            },
            INITIAL_CONTENT,
        );
        let tree = fixture.path("contract-delete-tree");
        file_system
            .delete(
                &tree,
                DeleteOptions {
                    recursive: true,
                    ..DeleteOptions::default()
                },
            )
            .expect("advertised recursive deletion must remove the tree");
        assert!(
            !file_system
                .exists(&tree)
                .expect("exists must inspect the deleted tree"),
            "recursively deleted trees must not remain present",
        );
        assert!(
            !file_system
                .exists(&child)
                .expect("exists must inspect recursively deleted children"),
            "recursively deleted children must not remain present",
        );
    }
}

/// Checks rename conflict handling, publication, and source removal.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture supporting read, write, and rename.
///
/// # Panics
///
/// Panics when rename overwrites without permission, leaves the source present,
/// fails to publish the exact source bytes, or downgrades an advertised
/// required atomic rename.
#[track_caller]
pub fn assert_rename_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    require_capability(file_system, FileSystemCapability::Rename);
    let source = fixture.path("contract-rename-source.bin");
    let destination = fixture.path("contract-rename-destination.bin");
    write_bytes(
        file_system,
        &source,
        WriteOptions::default(),
        INITIAL_CONTENT,
    );
    write_bytes(
        file_system,
        &destination,
        WriteOptions::default(),
        REPLACEMENT_CONTENT,
    );
    let error = file_system
        .rename(&source, &destination, RenameOptions::default())
        .expect_err("rename must reject an existing destination by default");
    assert_error_with_target(
        &error,
        FsErrorKind::AlreadyExists,
        FsOperation::Rename,
        Some(&source),
        Some(&destination),
        Some(file_system.info().provider_id()),
        None,
    );
    assert_content(file_system, &source, INITIAL_CONTENT);
    assert_content(file_system, &destination, REPLACEMENT_CONTENT);

    let atomicity = if file_system
        .capabilities()
        .contains(FileSystemCapability::AtomicRename)
    {
        AtomicityRequirement::Required
    } else {
        AtomicityRequirement::Preferred
    };
    let outcome = file_system
        .rename(
            &source,
            &destination,
            RenameOptions {
                overwrite: true,
                atomicity,
            },
        )
        .expect("rename with overwrite must publish the contract destination");
    if atomicity == AtomicityRequirement::Required {
        assert_eq!(
            AchievedAtomicity::Atomic,
            outcome.atomicity,
            "required atomic rename must report atomic publication",
        );
    }
    assert!(
        !file_system
            .exists(&source)
            .expect("exists must inspect the renamed source"),
        "rename must remove the source",
    );
    assert_content(file_system, &destination, INITIAL_CONTENT);
}

/// Checks file and tree copy preservation, conflict policy, and outcome
/// statistics.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture supporting read, write, and copy.
///
/// # Panics
///
/// Panics when copy changes the source, ignores a destination conflict policy,
/// publishes different destination bytes, or reports statistics inconsistent
/// with the copied payload.
#[track_caller]
pub fn assert_copy_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    require_capability(file_system, FileSystemCapability::Copy);
    let source = fixture.path("contract-copy-source.bin");
    let destination = fixture.path("contract-copy-destination.bin");
    write_bytes(
        file_system,
        &source,
        WriteOptions::default(),
        INITIAL_CONTENT,
    );
    let outcome = file_system
        .copy(&source, &destination, CopyOptions::file())
        .expect("copy must publish the contract destination");
    assert_content(file_system, &source, INITIAL_CONTENT);
    assert_content(file_system, &destination, INITIAL_CONTENT);

    assert_eq!(1, outcome.stats.files, "copy must report one copied file");
    assert_eq!(
        INITIAL_CONTENT.len() as u64,
        outcome.stats.bytes,
        "copy must report the copied byte count",
    );

    write_bytes(
        file_system,
        &destination,
        WriteOptions::default(),
        REPLACEMENT_CONTENT,
    );
    let error = file_system
        .copy(&source, &destination, CopyOptions::file())
        .expect_err("copy must reject an existing destination by default");
    assert_error_with_target(
        &error,
        FsErrorKind::AlreadyExists,
        FsOperation::Copy,
        Some(&source),
        Some(&destination),
        Some(file_system.info().provider_id()),
        None,
    );
    assert_content(file_system, &destination, REPLACEMENT_CONTENT);

    let skipped = file_system
        .copy(
            &source,
            &destination,
            CopyOptions {
                conflict: CopyConflictPolicy::Skip,
                ..CopyOptions::file()
            },
        )
        .expect("copy must support the skip conflict policy");
    assert_eq!(
        1, skipped.stats.skipped,
        "copy must report skipped conflicts"
    );
    assert_content(file_system, &destination, REPLACEMENT_CONTENT);

    let overwritten = file_system
        .copy(
            &source,
            &destination,
            CopyOptions {
                conflict: CopyConflictPolicy::Overwrite,
                ..CopyOptions::file()
            },
        )
        .expect("copy must support the overwrite conflict policy");
    assert_eq!(
        1, overwritten.stats.overwritten,
        "copy must report overwritten destinations",
    );
    assert_content(file_system, &destination, INITIAL_CONTENT);

    let tree_source = fixture.path("contract-copy-tree-source");
    let tree_child = fixture.path("contract-copy-tree-source/child.bin");
    let tree_destination = fixture.path("contract-copy-tree-destination");
    let copied_child = fixture.path("contract-copy-tree-destination/child.bin");
    write_bytes(
        file_system,
        &tree_child,
        WriteOptions {
            create_parent: true,
            ..WriteOptions::default()
        },
        INITIAL_CONTENT,
    );
    let outcome = file_system
        .copy(&tree_source, &tree_destination, CopyOptions::tree())
        .expect("tree copy must publish the contract destination");
    assert!(
        outcome.stats.files >= 1,
        "tree copy must report copied files",
    );
    assert_content(file_system, &copied_child, INITIAL_CONTENT);
}

/// Checks that unsupported option requirements fail before provider I/O.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture whose synchronous preflight is checked.
///
/// # Panics
///
/// Panics when a provider reaches missing-resource I/O before reporting any
/// unadvertised read, write, delete, rename, or copy requirement.
#[track_caller]
pub fn assert_preflight_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    let capabilities = file_system.capabilities();
    if capabilities.contains(FileSystemCapability::Read) {
        assert_read_preflight(fixture);
    }
    if capabilities.contains(FileSystemCapability::Write) {
        assert_write_preflight(fixture);
    }
    if capabilities.contains(FileSystemCapability::Delete) {
        assert_delete_preflight(fixture);
    }
    if capabilities.contains(FileSystemCapability::Rename) {
        assert_rename_preflight(fixture);
    }
    if capabilities.contains(FileSystemCapability::Copy) {
        assert_copy_preflight(fixture);
    }
}

/// Checks every optional read requirement against a missing path.
#[track_caller]
fn assert_read_preflight(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    let cases = [
        (
            FileSystemCapability::RangeRead,
            ReadOptions {
                offset: Some(1),
                ..ReadOptions::default()
            },
        ),
        (
            FileSystemCapability::ConditionalRead,
            ReadOptions {
                if_match: Some(ResourceVersion::from("contract-version")),
                ..ReadOptions::default()
            },
        ),
        (
            FileSystemCapability::ChecksumValidation,
            ReadOptions {
                checksum: ChecksumPolicy::Required,
                ..ReadOptions::default()
            },
        ),
    ];
    for (capability, options) in cases {
        if file_system.capabilities().contains(capability) {
            continue;
        }
        let path =
            fixture.path(&format!("contract-preflight-{capability:?}.bin"));
        let error = file_system
            .open_reader(&path, options)
            .expect_err("read requirements must fail before provider I/O");
        assert_error(
            &error,
            FsErrorKind::RequirementNotMet,
            FsOperation::OpenReader,
            Some(&path),
            Some(file_system.info().provider_id()),
            Some(capability),
        );
    }
}

/// Checks every optional write requirement against a missing path.
#[track_caller]
fn assert_write_preflight(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    let cases = [
        (
            FileSystemCapability::Append,
            WriteOptions {
                disposition: WriteDisposition::Append,
                atomicity: AtomicityRequirement::NotRequired,
                ..WriteOptions::default()
            },
        ),
        (
            FileSystemCapability::ConditionalWrite,
            WriteOptions {
                precondition: WritePrecondition::IfAbsent,
                ..WriteOptions::default()
            },
        ),
        (
            FileSystemCapability::AtomicReplace,
            WriteOptions {
                atomicity: AtomicityRequirement::Required,
                ..WriteOptions::default()
            },
        ),
    ];
    for (capability, options) in cases {
        if file_system.capabilities().contains(capability) {
            continue;
        }
        let path =
            fixture.path(&format!("contract-preflight-{capability:?}.bin"));
        let error = file_system
            .open_writer(&path, options)
            .expect_err("write requirements must fail before provider I/O");
        assert_error(
            &error,
            FsErrorKind::RequirementNotMet,
            FsOperation::OpenWriter,
            Some(&path),
            Some(file_system.info().provider_id()),
            Some(capability),
        );
    }
}

/// Checks optional delete requirements against a missing path.
#[track_caller]
fn assert_delete_preflight(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    let cases = [
        (
            FileSystemCapability::RecursiveDelete,
            DeleteOptions {
                recursive: true,
                ..DeleteOptions::default()
            },
        ),
        (
            FileSystemCapability::ConditionalDelete,
            DeleteOptions {
                if_match: Some(ResourceVersion::from("contract-version")),
                ..DeleteOptions::default()
            },
        ),
    ];
    for (capability, options) in cases {
        if file_system.capabilities().contains(capability) {
            continue;
        }
        let path = fixture.path(&format!("contract-preflight-{capability:?}"));
        let error = file_system
            .delete(&path, options)
            .expect_err("delete requirements must fail before provider I/O");
        assert_error(
            &error,
            FsErrorKind::RequirementNotMet,
            FsOperation::Delete,
            Some(&path),
            Some(file_system.info().provider_id()),
            Some(capability),
        );
    }
}

/// Checks required rename atomicity against a missing path.
#[track_caller]
fn assert_rename_preflight(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    if file_system
        .capabilities()
        .contains(FileSystemCapability::AtomicRename)
    {
        return;
    }
    let source = fixture.path("contract-preflight-rename-source.bin");
    let destination = fixture.path("contract-preflight-rename-destination.bin");
    let options = RenameOptions {
        atomicity: AtomicityRequirement::Required,
        ..RenameOptions::default()
    };
    let error = file_system
        .rename(&source, &destination, options)
        .expect_err("rename requirements must fail before provider I/O");
    assert_error(
        &error,
        FsErrorKind::RequirementNotMet,
        FsOperation::Rename,
        Some(&source),
        Some(file_system.info().provider_id()),
        Some(FileSystemCapability::AtomicRename),
    );
}

/// Checks required server-side copy against a missing path.
#[track_caller]
fn assert_copy_preflight(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    if file_system
        .capabilities()
        .contains(FileSystemCapability::ServerSideCopy)
    {
        return;
    }
    let source = fixture.path("contract-preflight-copy-source.bin");
    let destination = fixture.path("contract-preflight-copy-destination.bin");
    let options = CopyOptions {
        server_side: ServerSidePreference::Require,
        ..CopyOptions::file()
    };
    let error = file_system
        .copy(&source, &destination, options)
        .expect_err("copy requirements must fail before provider I/O");
    assert_error(
        &error,
        FsErrorKind::RequirementNotMet,
        FsOperation::Copy,
        Some(&source),
        Some(file_system.info().provider_id()),
        Some(FileSystemCapability::ServerSideCopy),
    );
}

/// Checks that a written resource has a kind compatible with path semantics.
///
/// # Parameters
///
/// * `semantics` - Path semantics advertised by the filesystem.
/// * `kind` - Provider-reported resource kind.
/// * `message` - Assertion message explaining the violated contract.
///
/// # Panics
///
/// Panics when the kind cannot represent a written file or object under the
/// advertised path semantics.
#[track_caller]
fn assert_file_like_kind(
    semantics: PathSemantics,
    kind: &FileKind,
    message: &str,
) {
    let matches = match semantics {
        PathSemantics::Hierarchical => *kind == FileKind::File,
        PathSemantics::ObjectKey => {
            matches!(kind, FileKind::File | FileKind::Object)
        }
        PathSemantics::ProviderSpecific => {
            matches!(
                kind,
                FileKind::File | FileKind::Object | FileKind::Other(_)
            )
        }
    };
    assert!(matches, "{message}: found {kind:?}");
}

/// Requires one capability before a positive contract is executed.
///
/// # Parameters
///
/// * `file_system` - Filesystem under test.
/// * `capability` - Capability required by the contract.
///
/// # Panics
///
/// Panics when the filesystem does not advertise the required capability.
#[track_caller]
pub(crate) fn require_capability(
    file_system: &dyn FileSystem,
    capability: FileSystemCapability,
) {
    assert!(
        file_system.capabilities().contains(capability),
        "{capability:?} is required by this contract",
    );
}

/// Writes and commits one payload with explicit options.
///
/// # Parameters
///
/// * `file_system` - Filesystem receiving the payload.
/// * `path` - Destination path.
/// * `options` - Requested write behavior.
/// * `bytes` - Complete payload.
///
/// # Returns
///
/// The provider-reported write outcome.
///
/// # Panics
///
/// Panics when the writer cannot open, accept bytes, or commit.
#[track_caller]
pub(crate) fn write_bytes(
    file_system: &dyn FileSystem,
    path: &FsPath,
    options: WriteOptions,
    bytes: &[u8],
) -> WriteOutcome {
    let mut writer = file_system
        .open_writer(path, options)
        .expect("the contract writer must open");
    if let Err(error) = writer.write_fully(bytes) {
        let _ = writer.abort();
        panic!("the contract writer must accept all bytes: {error}");
    }
    writer.commit().expect("the contract writer must commit")
}

/// Prepares a complete file for a capability-specific contract.
///
/// # Parameters
///
/// * `fixture` - Provider fixture that may offer out-of-band setup.
/// * `relative` - Testkit-relative destination path.
/// * `bytes` - Complete contents to prepare.
///
/// # Returns
/// The provider-local seeded path.
///
/// # Panics
///
/// Panics when the fixture has no seed hook and the filesystem does not
/// advertise ordinary write support, or when the ordinary write fails.
#[track_caller]
pub(crate) fn seed_file(
    fixture: &dyn FileSystemFixture,
    relative: &str,
    bytes: &[u8],
) -> FsPath {
    if let Some(path) = fixture.seed_file(relative, bytes) {
        return path;
    }
    let file_system = fixture.file_system();
    require_capability(file_system, FileSystemCapability::Write);
    let path = fixture.path(relative);
    write_bytes(file_system, &path, WriteOptions::default(), bytes);
    path
}

/// Checks resource contents when the fixture or filesystem can observe them.
///
/// # Parameters
///
/// * `fixture` - Provider fixture that may offer out-of-band observation.
/// * `path` - Provider-local resource path.
/// * `expected` - Expected complete contents.
///
/// # Panics
///
/// Panics when available observation returns bytes that differ from `expected`.
#[track_caller]
pub(crate) fn assert_observable_content(
    fixture: &dyn FileSystemFixture,
    path: &FsPath,
    expected: &[u8],
) {
    if let Some(actual) = fixture.read_file(path) {
        assert_eq!(expected, actual.as_slice(), "committed bytes must match");
    } else if fixture
        .file_system()
        .capabilities()
        .contains(FileSystemCapability::Read)
    {
        assert_content(fixture.file_system(), path, expected);
    }
}

/// Checks committed resource contents through the public read API.
///
/// # Parameters
///
/// * `file_system` - Filesystem containing the resource.
/// * `path` - Resource path.
/// * `expected` - Expected complete contents.
///
/// # Panics
///
/// Panics when the resource cannot be read or its bytes differ.
#[track_caller]
pub(crate) fn assert_content(
    file_system: &dyn FileSystem,
    path: &FsPath,
    expected: &[u8],
) {
    let actual = file_system
        .read_all(path, expected.len())
        .expect("the committed contract resource must be readable");
    assert_eq!(expected, actual.as_slice(), "committed bytes must match");
}

/// Checks a provider-reported byte count when it is known.
///
/// # Parameters
///
/// * `outcome` - Successful provider write outcome.
/// * `expected` - Number of bytes accepted by the session.
///
/// # Panics
///
/// Panics when a known byte count differs from the accepted payload size.
#[track_caller]
fn assert_bytes_written(outcome: &WriteOutcome, expected: usize) {
    if let Some(actual) = outcome.bytes_written {
        assert_eq!(
            expected as u64, actual,
            "known bytes_written must match the accepted payload",
        );
    }
}
