// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Contract assertions for advertised optional synchronous capabilities.

use qubit_fs::{
    ChecksumPolicy,
    CopyMethod,
    CopyOptions,
    DeleteOptions,
    FileReader,
    FileSystem,
    FileSystemCapability,
    FileSystemLimit,
    FsErrorKind,
    FsOperation,
    FsPath,
    ReadOptions,
    ServerSidePreference,
    WriteOptions,
    WritePrecondition,
};
use qubit_io::Input;

use crate::{
    FileSystemFixture,
    internal::assert_error,
    io_contract::{
        assert_content,
        require_capability,
        write_bytes,
    },
};

const CONTRACT_CONTENT: &[u8] = b"0123456789";
const FIRST_CONDITIONAL_TOKEN: &str = "qubit-fs-testkit-condition-a";
const SECOND_CONDITIONAL_TOKEN: &str = "qubit-fs-testkit-condition-b";

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
pub fn assert_range_read_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    if !file_system
        .capabilities()
        .contains(FileSystemCapability::RangeRead)
    {
        return;
    }
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    let path = fixture.path("contract-range-read.bin");
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
            offset: Some(2),
            length: Some(4),
            ..ReadOptions::default()
        },
    );
    assert_eq!(
        b"2345",
        bytes.as_slice(),
        "range reads must honor offset and length"
    );

    if let FileSystemLimit::Maximum(maximum) =
        file_system.limits().max_read_range_bytes()
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
pub fn assert_conditional_read_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    if !file_system
        .capabilities()
        .contains(FileSystemCapability::ConditionalRead)
    {
        return;
    }
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    let path = fixture.path("contract-conditional-read.bin");
    write_bytes(
        file_system,
        &path,
        WriteOptions::default(),
        CONTRACT_CONTENT,
    );

    let first = file_system.open_reader(
        &path,
        ReadOptions {
            if_none_match: Some(FIRST_CONDITIONAL_TOKEN.to_owned()),
            ..ReadOptions::default()
        },
    );
    match first {
        Ok(reader) => assert_eq!(
            CONTRACT_CONTENT,
            read_reader(reader).as_slice(),
            "conditional reads must preserve resource bytes",
        ),
        Err(error) => {
            assert_eq!(
                FsErrorKind::PreconditionFailed,
                error.kind(),
                "a rejected conditional read must report a precondition failure",
            );
            let bytes = read_bytes(
                file_system,
                &path,
                ReadOptions {
                    if_none_match: Some(SECOND_CONDITIONAL_TOKEN.to_owned()),
                    ..ReadOptions::default()
                },
            );
            assert_eq!(
                CONTRACT_CONTENT,
                bytes.as_slice(),
                "one of two distinct conditional-read tokens must permit the read",
            );
        }
    }

    if let Some(etag) = file_system
        .stat(&path)
        .expect("conditional-read metadata must remain readable")
        .etag
    {
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
                    if_match: Some(format!("{etag}-mismatch")),
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
    }
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
pub fn assert_checksum_validation_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    if !file_system
        .capabilities()
        .contains(FileSystemCapability::ChecksumValidation)
    {
        return;
    }
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    let path = fixture.path("contract-checksum-read.bin");
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
pub fn assert_conditional_write_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    if !file_system
        .capabilities()
        .contains(FileSystemCapability::ConditionalWrite)
    {
        return;
    }
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    let path = fixture.path("contract-conditional-write.bin");
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
pub fn assert_conditional_delete_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    if !file_system
        .capabilities()
        .contains(FileSystemCapability::ConditionalDelete)
    {
        return;
    }
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    require_capability(file_system, FileSystemCapability::Delete);
    let path = fixture.path("contract-conditional-delete.bin");
    write_bytes(
        file_system,
        &path,
        WriteOptions::default(),
        CONTRACT_CONTENT,
    );
    let etag = file_system
        .stat(&path)
        .expect("conditional-delete metadata must remain readable")
        .etag;
    let mismatch = etag.as_deref().map_or_else(
        || FIRST_CONDITIONAL_TOKEN.to_owned(),
        |etag| format!("{etag}-mismatch"),
    );
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

    if let Some(etag) = etag {
        file_system
            .delete(
                &path,
                DeleteOptions {
                    if_match: Some(etag),
                    ..DeleteOptions::default()
                },
            )
            .expect("matching ETags must allow conditional deletes");
    } else {
        file_system
            .delete(&path, DeleteOptions::default())
            .expect("ordinary cleanup must delete the contract resource");
    }
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
pub fn assert_server_side_copy_contract(fixture: &dyn FileSystemFixture) {
    let file_system = fixture.file_system();
    if !file_system
        .capabilities()
        .contains(FileSystemCapability::ServerSideCopy)
    {
        return;
    }
    require_capability(file_system, FileSystemCapability::Read);
    require_capability(file_system, FileSystemCapability::Write);
    require_capability(file_system, FileSystemCapability::Copy);
    let source = fixture.path("contract-server-copy-source.bin");
    let destination = fixture.path("contract-server-copy-destination.bin");
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
fn read_bytes(
    file_system: &dyn FileSystem,
    path: &FsPath,
    options: ReadOptions,
) -> Vec<u8> {
    let reader = file_system
        .open_reader(path, options)
        .expect("the contract reader must open");
    read_reader(reader)
}
