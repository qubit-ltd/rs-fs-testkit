// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Contract assertions for advertised optional synchronous capabilities.

use qubit_fs::{
    ChecksumPolicy, CopyMethod, CopyOptions, DeleteOptions, FileReader, FileSystem,
    FileSystemCapability, FileSystemLimit, FsErrorKind, FsOperation, FsPath, ReadOptions,
    ResourceVersion, ServerSidePreference, WriteOptions, WritePrecondition,
};
use qubit_io::Input;

use crate::{
    FileSystemFixture,
    internal::assert_error,
    io_contract::{assert_content, require_capability, write_bytes},
};

const CONTRACT_CONTENT: &[u8] = b"0123456789";
const FIRST_CONDITIONAL_TOKEN: &str = "qubit-fs-testkit-condition-a";
const UPDATED_CONTENT: &[u8] = b"conditional-write-updated";

/// Checks advertised byte-range reads.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture whose range-read capability is checked.
///
/// # Panics
///
/// Panics when an advertised range read returns wrong bytes or a finite
/// provider range limit is not enforced before reading.
#[track_caller]
pub fn assert_range_read_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    let path = fixture.path("contract-range-read.bin");
    if !file_system
        .capabilities()
        .contains(FileSystemCapability::RangeRead)
    {
        assert_missing_read_requirement(
            file_system,
            &path,
            FileSystemCapability::RangeRead,
            ReadOptions {
                offset: Some(1),
                ..ReadOptions::default()
            },
        );
        return;
    }
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    write_bytes(
        file_system,
        &path,
        WriteOptions::default(),
        CONTRACT_CONTENT,
    );

    let length = match file_system.limits().max_read_range_bytes() {
        FileSystemLimit::Maximum(maximum) => maximum.min(4),
        _ => 4,
    };
    let bytes = read_bytes(
        file_system,
        &path,
        ReadOptions {
            offset: Some(2),
            length: Some(length),
            ..ReadOptions::default()
        },
    );
    assert_eq!(
        &CONTRACT_CONTENT[2..2 + length as usize],
        bytes.as_slice(),
        "range reads must honor offset and length"
    );

    if let FileSystemLimit::Maximum(maximum) = file_system.limits().max_read_range_bytes()
        && maximum < u64::MAX
    {
        let error = file_system
            .open_reader(
                &path,
                ReadOptions {
                    length: Some(maximum + 1),
                    ..ReadOptions::default()
                },
            )
            .expect_err("ranges larger than a declared limit must be rejected");
        assert_error(
            &error,
            FsErrorKind::ResourceLimitExceeded,
            FsOperation::OpenReader,
            Some(&path),
            Some(file_system.info().provider_id()),
            None,
        );
    }
}

/// Checks advertised conditional reads.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture whose conditional-read capability is
///   checked.
///
/// # Panics
///
/// Panics when conditional reads are rejected despite being advertised, or
/// when ETag conditions have incorrect success or failure behavior.
#[track_caller]
pub fn assert_conditional_read_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    let path = fixture.path("contract-conditional-read.bin");
    if !file_system
        .capabilities()
        .contains(FileSystemCapability::ConditionalRead)
    {
        assert_missing_read_requirement(
            file_system,
            &path,
            FileSystemCapability::ConditionalRead,
            ReadOptions {
                if_match: Some(ResourceVersion::from(FIRST_CONDITIONAL_TOKEN)),
                ..ReadOptions::default()
            },
        );
        return;
    }
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    write_bytes(
        file_system,
        &path,
        WriteOptions::default(),
        CONTRACT_CONTENT,
    );

    let etag = file_system
        .stat(&path)
        .expect("conditional-read metadata must remain readable")
        .etag
        .expect("advertised conditional reads must expose an ETag");
    let bytes = read_bytes(
        file_system,
        &path,
        ReadOptions {
            if_match: Some(etag.clone()),
            ..ReadOptions::default()
        },
    );
    assert_eq!(
        CONTRACT_CONTENT,
        bytes.as_slice(),
        "matching ETags must allow reads"
    );
    let error = file_system
        .open_reader(
            &path,
            ReadOptions {
                if_match: Some(ResourceVersion::new(format!("{etag}-mismatch"))),
                ..ReadOptions::default()
            },
        )
        .expect_err("mismatched ETags must reject conditional reads");
    assert_error(
        &error,
        FsErrorKind::PreconditionFailed,
        FsOperation::OpenReader,
        Some(&path),
        Some(file_system.info().provider_id()),
        None,
    );
    let error = file_system
        .open_reader(
            &path,
            ReadOptions {
                if_none_match: Some(etag.clone()),
                ..ReadOptions::default()
            },
        )
        .expect_err("matching if-none-match ETags must reject reads");
    assert_error(
        &error,
        FsErrorKind::PreconditionFailed,
        FsOperation::OpenReader,
        Some(&path),
        Some(file_system.info().provider_id()),
        None,
    );
    let bytes = read_bytes(
        file_system,
        &path,
        ReadOptions {
            if_none_match: Some(ResourceVersion::new(format!("{etag}-mismatch"))),
            ..ReadOptions::default()
        },
    );
    assert_eq!(
        CONTRACT_CONTENT,
        bytes.as_slice(),
        "nonmatching if-none-match ETags must allow reads"
    );
}

/// Checks advertised checksum-required reads.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture whose checksum-validation capability is
///   checked.
///
/// # Panics
///
/// Panics when a required checksum read cannot be opened or does not return the
/// committed bytes.
#[track_caller]
pub fn assert_checksum_validation_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    let path = fixture.path("contract-checksum-read.bin");
    if !file_system
        .capabilities()
        .contains(FileSystemCapability::ChecksumValidation)
    {
        assert_missing_read_requirement(
            file_system,
            &path,
            FileSystemCapability::ChecksumValidation,
            ReadOptions {
                checksum: ChecksumPolicy::Required,
                ..ReadOptions::default()
            },
        );
        return;
    }
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    write_bytes(
        file_system,
        &path,
        WriteOptions::default(),
        CONTRACT_CONTENT,
    );
    let bytes = read_bytes(
        file_system,
        &path,
        ReadOptions {
            checksum: ChecksumPolicy::Required,
            ..ReadOptions::default()
        },
    );
    assert_eq!(
        CONTRACT_CONTENT,
        bytes.as_slice(),
        "checksum-required reads must preserve resource bytes",
    );
}

/// Checks advertised conditional writes.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture whose conditional-write capability is
///   checked.
///
/// # Panics
///
/// Panics when `IfAbsent` does not create a missing resource, overwrites an
/// existing resource, or reports an incorrect precondition error.
#[track_caller]
pub fn assert_conditional_write_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    let path = fixture.path("contract-conditional-write.bin");
    if !file_system
        .capabilities()
        .contains(FileSystemCapability::ConditionalWrite)
    {
        assert_missing_write_requirement(
            file_system,
            &path,
            WriteOptions {
                precondition: WritePrecondition::IfAbsent,
                ..WriteOptions::default()
            },
        );
        return;
    }
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    let options = WriteOptions {
        precondition: WritePrecondition::IfAbsent,
        ..WriteOptions::default()
    };
    write_bytes(file_system, &path, options.clone(), CONTRACT_CONTENT);
    let error = file_system
        .open_writer(&path, options)
        .expect_err("IfAbsent writes must reject an existing resource");
    assert_error(
        &error,
        FsErrorKind::PreconditionFailed,
        FsOperation::OpenWriter,
        Some(&path),
        Some(file_system.info().provider_id()),
        None,
    );
    assert_content(file_system, &path, CONTRACT_CONTENT);
    let etag = file_system
        .stat(&path)
        .expect("conditional-write metadata must remain readable")
        .etag
        .expect("advertised conditional writes must expose an ETag");
    let error = file_system
        .open_writer(
            &path,
            WriteOptions {
                precondition: WritePrecondition::IfMatch(ResourceVersion::new(format!(
                    "{etag}-mismatch"
                ))),
                ..WriteOptions::default()
            },
        )
        .expect_err("mismatched IfMatch writes must reject");
    assert_error(
        &error,
        FsErrorKind::PreconditionFailed,
        FsOperation::OpenWriter,
        Some(&path),
        Some(file_system.info().provider_id()),
        None,
    );
    assert_content(file_system, &path, CONTRACT_CONTENT);
    write_bytes(
        file_system,
        &path,
        WriteOptions {
            precondition: WritePrecondition::IfMatch(etag),
            ..WriteOptions::default()
        },
        UPDATED_CONTENT,
    );
    assert_content(file_system, &path, UPDATED_CONTENT);
}

/// Checks advertised conditional deletes.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture whose conditional-delete capability is
///   checked.
///
/// # Panics
///
/// Panics when a mismatched version does not preserve the resource, or when a
/// matching ETag is exposed but cannot delete the resource.
#[track_caller]
pub fn assert_conditional_delete_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    let path = fixture.path("contract-conditional-delete.bin");
    if !file_system
        .capabilities()
        .contains(FileSystemCapability::ConditionalDelete)
    {
        assert_missing_delete_requirement(
            file_system,
            &path,
            DeleteOptions {
                if_match: Some(ResourceVersion::from(FIRST_CONDITIONAL_TOKEN)),
                ..DeleteOptions::default()
            },
        );
        return;
    }
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    require_capability(file_system, FileSystemCapability::Delete);
    write_bytes(
        file_system,
        &path,
        WriteOptions::default(),
        CONTRACT_CONTENT,
    );
    let etag = file_system
        .stat(&path)
        .expect("conditional-delete metadata must remain readable")
        .etag
        .expect("advertised conditional deletes must expose an ETag");
    let mismatch = ResourceVersion::new(format!("{etag}-mismatch"));
    let error = file_system
        .delete(
            &path,
            DeleteOptions {
                if_match: Some(mismatch),
                ..DeleteOptions::default()
            },
        )
        .expect_err("mismatched versions must reject conditional deletes");
    assert_error(
        &error,
        FsErrorKind::PreconditionFailed,
        FsOperation::Delete,
        Some(&path),
        Some(file_system.info().provider_id()),
        None,
    );
    assert_content(file_system, &path, CONTRACT_CONTENT);

    file_system
        .delete(
            &path,
            DeleteOptions {
                if_match: Some(etag),
                ..DeleteOptions::default()
            },
        )
        .expect("matching ETags must allow conditional deletes");
    assert!(
        !file_system
            .exists(&path)
            .expect("exists must inspect conditionally deleted resources"),
        "matching ETags must remove the resource",
    );
}

/// Checks advertised required server-side copies.
///
/// # Parameters
///
/// * `fixture` - Fresh provider fixture whose server-side-copy capability is
///   checked.
///
/// # Panics
///
/// Panics when required server-side copies fail, report a non-server-side
/// method, or produce wrong destination bytes.
#[track_caller]
pub fn assert_server_side_copy_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    let source = fixture.path("contract-server-copy-source.bin");
    let destination = fixture.path("contract-server-copy-destination.bin");
    if !file_system
        .capabilities()
        .contains(FileSystemCapability::ServerSideCopy)
    {
        assert_missing_copy_requirement(
            file_system,
            &source,
            &destination,
            CopyOptions {
                server_side: ServerSidePreference::Require,
                ..CopyOptions::file()
            },
        );
        return;
    }
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    require_capability(file_system, FileSystemCapability::Copy);
    write_bytes(
        file_system,
        &source,
        WriteOptions::default(),
        CONTRACT_CONTENT,
    );
    let outcome = file_system
        .copy(
            &source,
            &destination,
            CopyOptions {
                server_side: ServerSidePreference::Require,
                ..CopyOptions::file()
            },
        )
        .expect("required server-side copies must succeed");
    assert_eq!(
        CopyMethod::ServerSide,
        outcome.method,
        "required server-side copies must report the server-side method",
    );
    assert_content(file_system, &destination, CONTRACT_CONTENT);
}

/// Checks a missing read capability before provider resource I/O.
#[track_caller]
fn assert_missing_read_requirement(
    file_system: &dyn FileSystem,
    path: &FsPath,
    capability: FileSystemCapability,
    options: ReadOptions,
) {
    let error = file_system
        .open_reader(path, options)
        .expect_err("missing read capabilities must reject before provider I/O");
    assert_error(
        &error,
        FsErrorKind::RequirementNotMet,
        FsOperation::OpenReader,
        Some(path),
        Some(file_system.info().provider_id()),
        Some(capability),
    );
}

/// Checks a missing conditional-write capability before provider resource I/O.
#[track_caller]
fn assert_missing_write_requirement(
    file_system: &dyn FileSystem,
    path: &FsPath,
    options: WriteOptions,
) {
    let error = file_system
        .open_writer(path, options)
        .expect_err("missing write capabilities must reject before provider I/O");
    assert_error(
        &error,
        FsErrorKind::RequirementNotMet,
        FsOperation::OpenWriter,
        Some(path),
        Some(file_system.info().provider_id()),
        Some(FileSystemCapability::ConditionalWrite),
    );
}

/// Checks a missing conditional-delete capability before provider resource I/O.
#[track_caller]
fn assert_missing_delete_requirement(
    file_system: &dyn FileSystem,
    path: &FsPath,
    options: DeleteOptions,
) {
    let error = file_system
        .delete(path, options)
        .expect_err("missing delete capabilities must reject before provider I/O");
    assert_error(
        &error,
        FsErrorKind::RequirementNotMet,
        FsOperation::Delete,
        Some(path),
        Some(file_system.info().provider_id()),
        Some(FileSystemCapability::ConditionalDelete),
    );
}

/// Checks a missing server-side-copy capability before provider resource I/O.
#[track_caller]
fn assert_missing_copy_requirement(
    file_system: &dyn FileSystem,
    source: &FsPath,
    destination: &FsPath,
    options: CopyOptions,
) {
    let error = file_system
        .copy(source, destination, options)
        .expect_err("missing copy capabilities must reject before provider I/O");
    assert_error(
        &error,
        FsErrorKind::RequirementNotMet,
        FsOperation::Copy,
        Some(source),
        Some(file_system.info().provider_id()),
        Some(FileSystemCapability::ServerSideCopy),
    );
}

/// Reads every byte from an already-opened contract reader.
///
/// # Parameters
///
/// * `reader` - Opened reader to drain.
///
/// # Returns
///
/// The complete bytes emitted by `reader`.
///
/// # Panics
///
/// Panics when the reader cannot return its complete content.
#[track_caller]
fn read_reader(mut reader: FileReader) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .expect("the contract reader must return all bytes");
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    bytes
}

/// Opens one reader with explicit options and returns its bytes.
///
/// # Parameters
///
/// * `file_system` - Filesystem containing the resource.
/// * `path` - Resource path.
/// * `options` - Requested read behavior.
///
/// # Returns
///
/// The complete bytes returned by the opened reader.
///
/// # Panics
///
/// Panics when the reader cannot open or cannot return all bytes.
#[track_caller]
fn read_bytes(file_system: &dyn FileSystem, path: &FsPath, options: ReadOptions) -> Vec<u8> {
    let reader = file_system
        .open_reader(path, options)
        .expect("the contract reader must open");
    read_reader(reader)
}
