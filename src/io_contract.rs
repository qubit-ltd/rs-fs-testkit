// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Contract assertions for synchronous filesystem I/O.

use qubit_fs::{
    AchievedAtomicity, AtomicityRequirement, FileKind, FileSystem, FileSystemCapability,
    FileSystemExt, FsErrorKind, FsOperation, FsPath, ReadOptions, WriteDisposition, WriteOptions,
    WriteOutcome,
};
use qubit_io::Output;

use crate::{FileSystemFixture, internal::assert_error};

const INITIAL_CONTENT: &[u8] = b"initial contract content";
const REPLACEMENT_CONTENT: &[u8] = b"replacement contract content";
const APPENDED_CONTENT: &[u8] = b" + appended";

/// Checks metadata and existence behavior for missing and written files.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture supporting read and write operations.
///
/// # Panics
///
/// Panics when missing-path errors lose context, existence disagrees with
/// metadata, or written-file metadata is inconsistent.
pub fn assert_stat_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    require_capability(file_system, FileSystemCapability::Read);
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
    assert_eq!(
        FileKind::File,
        metadata.kind,
        "written resources must be files"
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
        let outcome = write_bytes(file_system, &path, options, REPLACEMENT_CONTENT);
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

/// Checks that unsupported range requirements fail before provider I/O.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture whose read preflight is checked.
///
/// # Panics
///
/// Panics when a provider without range-read support reaches missing-resource
/// I/O before reporting the required capability.
pub fn assert_preflight_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    require_capability(file_system, FileSystemCapability::Read);
    if file_system
        .capabilities()
        .contains(FileSystemCapability::RangeRead)
    {
        return;
    }
    let path = fixture.path("contract-preflight-missing.bin");
    let options = ReadOptions {
        offset: Some(1),
        ..ReadOptions::default()
    };
    let error = file_system
        .open_reader(&path, options)
        .expect_err("range requirements must fail before provider I/O");
    assert_error(
        &error,
        FsErrorKind::RequirementNotMet,
        FsOperation::OpenReader,
        Some(&path),
        Some(file_system.info().provider_id()),
        Some(FileSystemCapability::RangeRead),
    );
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
fn require_capability(file_system: &dyn FileSystem, capability: FileSystemCapability) {
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
fn write_bytes(
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
fn assert_content(file_system: &dyn FileSystem, path: &FsPath, expected: &[u8]) {
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
fn assert_bytes_written(outcome: &WriteOutcome, expected: usize) {
    if let Some(actual) = outcome.bytes_written {
        assert_eq!(
            expected as u64, actual,
            "known bytes_written must match the accepted payload",
        );
    }
}
