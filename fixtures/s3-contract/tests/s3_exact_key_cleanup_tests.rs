// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod common;

use std::error::Error;
use std::io;
use std::panic;
use std::sync::Arc;

use bytes::Bytes;
use futures_util::FutureExt;
use object_store::ObjectStore;
use object_store::ObjectStoreExt;
use object_store::memory::InMemory;
use object_store::path::Path;

async fn put(store: &dyn ObjectStore, key: &Path) {
    store.put(key, Bytes::from_static(b"owned").into()).await.unwrap();
}

async fn exists(store: &dyn ObjectStore, key: &Path) -> bool {
    store.head(key).await.is_ok()
}

#[tokio::test]
async fn test_successful_verification_cleans_owned_keys_and_keeps_sentinel() {
    let store = Arc::new(InMemory::new());
    let owned = [Path::from("owned/a"), Path::from("owned/b")];
    let sentinel = Path::from("other/sentinel");
    for key in owned.iter().chain(std::iter::once(&sentinel)) {
        put(&store, key).await;
    }

    let (outcome, errors) =
        common::exact_key_cleanup::run_with_exact_key_cleanup(&store, &owned, async { Ok(()) }).await;
    assert!(outcome.is_ok_and(|result| result.is_ok()));
    assert!(errors.is_empty());
    for key in &owned {
        assert!(!exists(&store, key).await);
    }
    assert!(exists(&store, &sentinel).await);
}

#[tokio::test]
async fn test_verification_error_and_not_found_still_clean_all_exact_keys() {
    let store = Arc::new(InMemory::new());
    let owned = [
        Path::from("owned/fail-delete"),
        Path::from("owned/present"),
        Path::from("owned/already-gone"),
    ];
    let sentinel = Path::from("unrelated/sentinel");
    put(&store, &owned[0]).await;
    put(&store, &owned[1]).await;
    put(&store, &sentinel).await;

    let absent = owned[2].clone();
    let failing = owned[0].clone();
    let store_for_delete = Arc::clone(&store);
    let (outcome, errors) = common::exact_key_cleanup::run_with_exact_key_cleanup_using(
        &owned,
        async { Err(Box::new(io::Error::other("verification marker")) as Box<dyn Error>) },
        |key| {
            let absent = absent.clone();
            let failing = failing.clone();
            let store = Arc::clone(&store_for_delete);
            async move {
                if key == failing {
                    return Err(object_store::Error::Generic {
                        store: "test",
                        source: Box::new(io::Error::other("injected delete failure")),
                    });
                }
                if key == absent {
                    return Err(object_store::Error::NotFound {
                        path: key.to_string(),
                        source: Box::new(io::Error::new(io::ErrorKind::NotFound, "already absent")),
                    });
                }
                store.delete(&key).await
            }
        },
    )
    .await;
    assert!(outcome.as_ref().is_ok_and(|result| {
        result
            .as_ref()
            .is_err_and(|error| error.to_string() == "verification marker")
    }));
    assert_eq!(errors.len(), 1);
    assert!(exists(&store, &owned[0]).await);
    assert!(!exists(&store, &owned[1]).await);
    assert!(exists(&store, &sentinel).await);

    let panic = panic::AssertUnwindSafe(async {
        common::exact_key_cleanup::finalize_verification(outcome, errors);
    })
    .catch_unwind()
    .await
    .expect_err("verification error must remain a failure");
    let message = panic.downcast_ref::<String>().unwrap();
    assert!(message.contains("verification marker"));
    assert!(message.contains("cleanup failed for 1 owned keys"));
}

#[tokio::test]
async fn test_panic_is_resumed_after_cleanup_and_failed_delete_does_not_stop_loop() {
    let base = Arc::new(InMemory::new());
    let failing = Path::from("owned/fail-delete");
    let remaining = Path::from("owned/delete-me");
    let sentinel = Path::from("unrelated/sentinel");
    for key in [&failing, &remaining, &sentinel] {
        put(base.as_ref(), key).await;
    }
    let base_for_delete = Arc::clone(&base);
    let failing_for_delete = failing.clone();
    let (outcome, errors) = common::exact_key_cleanup::run_with_exact_key_cleanup_using(
        &[failing.clone(), remaining.clone()],
        async { panic::panic_any(0xF57u32) },
        move |key| {
            let base = Arc::clone(&base_for_delete);
            let failing = failing_for_delete.clone();
            async move {
                if key == failing {
                    return Err(object_store::Error::Generic {
                        store: "test",
                        source: Box::new(io::Error::other("injected delete failure")),
                    });
                }
                base.delete(&key).await
            }
        },
    )
    .await;
    assert_eq!(errors.len(), 1);
    assert!(exists(base.as_ref(), &failing).await);
    assert!(!exists(base.as_ref(), &remaining).await);
    assert!(exists(base.as_ref(), &sentinel).await);

    let resumed = panic::AssertUnwindSafe(async {
        common::exact_key_cleanup::finalize_verification(outcome, errors);
    })
    .catch_unwind()
    .await
    .expect_err("original verification panic must resume");
    assert_eq!(resumed.downcast_ref::<u32>(), Some(&0xF57));
}

#[tokio::test]
async fn test_successful_verification_with_cleanup_failure_fails() {
    let base = Arc::new(InMemory::new());
    let failing = Path::from("owned/fail-delete");
    put(base.as_ref(), &failing).await;
    let failing_for_delete = failing.clone();
    let (outcome, errors) = common::exact_key_cleanup::run_with_exact_key_cleanup_using(
        &[Path::from("owned/fail-delete")],
        async { Ok(()) },
        move |key| {
            let base = Arc::clone(&base);
            let failing = failing_for_delete.clone();
            async move {
                if key == failing {
                    return Err(object_store::Error::Generic {
                        store: "test",
                        source: Box::new(io::Error::other("injected delete failure")),
                    });
                }
                base.delete(&key).await
            }
        },
    )
    .await;
    assert_eq!(errors.len(), 1);
    let panic = panic::AssertUnwindSafe(async {
        common::exact_key_cleanup::finalize_verification(outcome, errors);
    })
    .catch_unwind()
    .await
    .expect_err("cleanup failure must fail successful verification");
    assert!(
        panic
            .downcast_ref::<String>()
            .unwrap()
            .contains("cleanup failed for 1 owned keys")
    );
}
