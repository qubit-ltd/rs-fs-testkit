use qubit_fs::Path;
use qubit_fs::directory::ListFilter;
use qubit_fs::directory::ListOptions;
use qubit_fs::directory::ListScope;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteOptions;
use qubit_fs_s3_contract::open_in_memory;
mod common;

/// The SDK component prefix must not omit matching raw logical keys.
#[tokio::test]
async fn raw_scope_and_relative_filter_select_exact_keys() {
    let filesystem = open_in_memory("raw-list").unwrap();
    for key in ["folder", "folder/a", "folder/ab", "folder/b", "folderish", "other"] {
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
        let options = ListOptions::object_keys().with_filter(Some(ListFilter::LiteralPrefix(filter.into())));
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
fn ambiguous_sdk_resource_keys_are_rejected() {
    for key in ["a/", "/a", "a//b", "a/./b", "a/../b"] {
        assert!(qubit_fs_s3_contract::validate_key(key).is_err(), "{key}");
    }
}

/// Whole-namespace listing includes unrelated keys without exposing SDK
/// prefixes.
#[tokio::test]
async fn namespace_is_exact_and_scoped_to_configuration() {
    use object_store::ObjectStoreExt;
    use qubit_fs_testkit::AsyncFileSystemFixture;
    let fixture = common::Fixture::memory("namespace-boundary");
    for key in ["top", "folder/a", "folderish"] {
        assert!(matches!(
            fixture.seed_file(key, b"x").await.unwrap(),
            qubit_fs_testkit::FixtureSupport::Supported(_)
        ));
    }
    fixture
        .store
        .put(
            &object_store::path::Path::from("outside/leak"),
            bytes::Bytes::from_static(b"x").into(),
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

/// Scan budgets count unmatched SDK entries rather than only returned entries.
#[tokio::test]
async fn unmatched_entries_still_exhaust_scan_budget() {
    use qubit_fs::error::FsErrorKind;
    use qubit_fs_testkit::AsyncFileSystemFixture;
    let fixture = common::Fixture::memory("scan-budget");
    for index in 0..1025 {
        assert!(matches!(
            fixture.seed_file(&format!("decoy-{index:04}"), b"x").await.unwrap(),
            qubit_fs_testkit::FixtureSupport::Supported(_)
        ));
    }
    let options = ListOptions::object_keys().with_filter(Some(ListFilter::LiteralPrefix("no-match".into())));
    let mut stream = fixture.filesystem.list(&ListScope::Namespace, options).await.unwrap();
    assert_eq!(
        stream.next_entry_async().await.unwrap_err().kind(),
        FsErrorKind::ResourceLimitExceeded
    );
}

/// Unsupported resource spellings are rejected by every resource operation.
#[tokio::test]
async fn ambiguous_resources_are_rejected_before_sdk_dispatch() {
    use qubit_fs::error::FsErrorKind;
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
            filesystem.open_writer(&path, options).await.unwrap_err().kind(),
            FsErrorKind::InvalidPath
        );
    }
}
