// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use bytes::Bytes;
use object_store::ObjectStoreExt;
use object_store::path::Path as ObjectPath;
use qubit_fs::Path;
use qubit_fs::directory::ListFilter;
use qubit_fs::directory::ListOptions;
use qubit_fs::directory::ListScope;
use qubit_fs::error::FsErrorKind;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteOptions;
use qubit_fs_s3_contract::open_in_memory;
use qubit_fs_s3_contract::validate_key;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::FixtureSupport;

mod common;

/// The SDK component prefix must not omit matching raw logical keys.
#[tokio::test]
async fn test_raw_scope_and_relative_filter_select_exact_keys() {
    let filesystem = open_in_memory("raw-list").unwrap();
    for key in [
        "folder",
        "folder/a",
        "folder/ab",
        "folder/b",
        "folderish",
        "other",
    ] {
        let options = WriteOptions::default().with_disposition(WriteDisposition::CreateNew);
        filesystem
            .begin_write_all(Path::parse_literal(key).unwrap(), b"x".to_vec(), options)
            .unwrap()
            .execute()
            .await
            .unwrap();
    }
    for (scope, filter, expected) in [
        (
            ListScope::Path(Path::parse_literal("folder").unwrap()),
            "",
            vec!["folder", "folder/a", "folder/ab", "folder/b", "folderish"],
        ),
        (
            ListScope::Path(Path::parse_literal("folder/").unwrap()),
            "a",
            vec!["folder/a", "folder/ab"],
        ),
    ] {
        let options = ListOptions::object_keys()
            .with_filter(Some(ListFilter::LiteralPrefix(filter.into())));
        let mut stream = filesystem.list(&scope, options).await.unwrap();
        let mut actual = Vec::new();
        while let Some(entry) = stream.next_entry_async().await.unwrap() {
            actual.push(entry.path.as_str().to_owned());
        }
        actual.sort();
        assert_eq!(actual, expected);
    }
}

/// Resource keys must round-trip through the SDK without changing identity.
#[test]
fn test_ambiguous_sdk_resource_keys_are_rejected() {
    for key in ["a/", "/a", "a//b", "a/./b", "a/../b"] {
        assert!(validate_key(key).is_err(), "{key}");
    }
}

/// Whole-namespace listing includes unrelated keys without exposing SDK
/// prefixes.
#[tokio::test]
async fn test_namespace_is_exact_and_scoped_to_configuration() {
    let fixture = common::Fixture::memory("namespace-boundary");
    for key in ["top", "folder/a", "folderish"] {
        assert!(matches!(
            fixture.seed_file(key, b"x").await.unwrap(),
            FixtureSupport::Supported(_)
        ));
    }
    fixture
        .store
        .put(
            &ObjectPath::from("outside/leak"),
            Bytes::from_static(b"x").into(),
        )
        .await
        .unwrap();
    let mut stream = fixture
        .filesystem
        .list(&ListScope::Namespace, ListOptions::object_keys())
        .await
        .unwrap();
    let mut actual = Vec::new();
    while let Some(entry) = stream.next_entry_async().await.unwrap() {
        actual.push(entry.path.as_str().to_owned());
    }
    actual.sort();
    assert_eq!(actual, ["folder/a", "folderish", "top"]);
}

#[tokio::test]
async fn test_namespace_snapshot_does_not_claim_unowned_keys() {
    let fixture = common::Fixture::memory("snapshot-ownership");
    let sentinel = ObjectPath::from(format!("{}/sentinel", fixture.config.prefix));
    fixture
        .store
        .put(&sentinel, Bytes::from_static(b"keep").into())
        .await
        .unwrap();
    assert!(matches!(
        fixture.seed_file("owned", b"remove").await.unwrap(),
        FixtureSupport::Supported(_)
    ));
    let owned = ObjectPath::from(format!("{}/owned", fixture.config.prefix));
    for _ in 0..2 {
        let FixtureSupport::Supported(paths) = fixture.snapshot_namespace_paths().await.unwrap()
        else {
            panic!("namespace snapshot must be supported");
        };
        let mut names: Vec<_> = paths.iter().map(|path| path.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["owned", "sentinel"]);
    }
    for _ in 0..2 {
        fixture.teardown().await.unwrap();
        assert!(matches!(
            fixture.store.head(&owned).await,
            Err(object_store::Error::NotFound { .. })
        ));
        let retained = fixture
            .store
            .get(&sentinel)
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        assert_eq!(retained.as_ref(), b"keep");
    }
}

/// Scan budgets count unmatched SDK entries rather than only returned entries.
#[tokio::test]
async fn test_unmatched_entries_still_exhaust_scan_budget() {
    let fixture = common::Fixture::memory("scan-budget");
    for index in 0..1025 {
        assert!(matches!(
            fixture.seed_file(&format!("decoy-{index:04}"), b"x").await.unwrap(),
            FixtureSupport::Supported(_)
        ));
    }
    let options = ListOptions::object_keys()
        .with_filter(Some(ListFilter::LiteralPrefix("no-match".into())));
    let mut stream = fixture.filesystem.list(&ListScope::Namespace, options).await.unwrap();
    assert_eq!(
        stream.next_entry_async().await.unwrap_err().kind(),
        FsErrorKind::ResourceLimitExceeded
    );
}

/// Unsupported resource spellings are rejected by every resource operation.
#[tokio::test]
async fn test_ambiguous_resources_are_rejected_before_sdk_dispatch() {
    let filesystem = open_in_memory("ambiguous-keys").unwrap();
    for key in ["a/", "/a", "a//b", "a/./b", "a/../b"] {
        let path = Path::parse_literal(key).unwrap();
        assert_eq!(
            filesystem.stat(&path).await.unwrap_err().kind(),
            FsErrorKind::InvalidPath
        );
        assert_eq!(
            filesystem
                .open_reader(&path, Default::default())
                .await
                .unwrap_err()
                .kind(),
            FsErrorKind::InvalidPath
        );
        let options = WriteOptions::default().with_disposition(WriteDisposition::CreateNew);
        assert_eq!(
            filesystem
                .open_writer(&path, options)
                .await
                .unwrap_err()
                .error()
                .kind(),
            FsErrorKind::InvalidPath
        );
    }
}
