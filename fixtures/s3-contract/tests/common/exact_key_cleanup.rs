// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cleanup for exact keys owned by a verification run.

use std::any::Any;
use std::collections::HashSet;
use std::error::Error;
use std::future::Future;
use std::panic;
use std::sync::Arc;
use std::sync::Mutex;

use futures_util::FutureExt;
use object_store::Error as ObjectStoreError;
use object_store::ObjectStore;
use object_store::ObjectStoreExt;
use object_store::path::Path as ObjectStorePath;

pub type VerificationOutcome = Result<Result<(), Box<dyn Error>>, Box<dyn Any + Send>>;

/// Captures verification failures and attempts deletion of every exact owned
/// key.
pub async fn run_with_exact_key_cleanup<F>(
    store: &dyn ObjectStore,
    owned: &[ObjectStorePath],
    verification: F,
) -> (VerificationOutcome, Vec<String>)
where
    F: Future<Output = Result<(), Box<dyn Error>>>,
{
    run_with_exact_key_cleanup_using(owned, verification, |key| async move {
        store.delete(&key).await
    })
    .await
}

/// Uses a ledger populated only after independent successful create results.
pub async fn run_with_exact_key_cleanup_ledger<F>(
    store: &dyn ObjectStore,
    owned: Arc<Mutex<HashSet<ObjectStorePath>>>,
    verification: F,
) -> (VerificationOutcome, Vec<String>)
where
    F: Future<Output = Result<(), Box<dyn Error>>>,
{
    run_with_exact_key_cleanup_provider(
        verification,
        move || {
            owned
                .lock()
                .expect("created-key ledger")
                .iter()
                .cloned()
                .collect()
        },
        |key| async move { store.delete(&key).await },
    )
    .await
}

/// Test seam for exercising deletion failures without changing production
/// stores.
pub async fn run_with_exact_key_cleanup_using<F, D, DF>(
    owned: &[ObjectStorePath],
    verification: F,
    delete: D,
) -> (VerificationOutcome, Vec<String>)
where
    F: Future<Output = Result<(), Box<dyn Error>>>,
    D: FnMut(ObjectStorePath) -> DF,
    DF: Future<Output = Result<(), ObjectStoreError>>,
{
    run_with_exact_key_cleanup_provider(verification, || owned.to_vec(), delete).await
}

async fn run_with_exact_key_cleanup_provider<F, P, D, DF>(
    verification: F,
    owned: P,
    mut delete: D,
) -> (VerificationOutcome, Vec<String>)
where
    F: Future<Output = Result<(), Box<dyn Error>>>,
    P: FnOnce() -> Vec<ObjectStorePath>,
    D: FnMut(ObjectStorePath) -> DF,
    DF: Future<Output = Result<(), ObjectStoreError>>,
{
    let outcome = panic::AssertUnwindSafe(verification).catch_unwind().await;
    let mut cleanup_errors = Vec::new();
    for key in owned() {
        match panic::AssertUnwindSafe(delete(key.clone()))
            .catch_unwind()
            .await
        {
            Ok(Ok(())) => {}
            Ok(Err(object_store::Error::NotFound { .. })) => {}
            Ok(Err(error)) => cleanup_errors.push(format!("{key}: {error}")),
            Err(payload) => cleanup_errors.push(format!(
                "{key}: delete panicked: {}",
                panic_message(&payload)
            )),
        }
    }
    (outcome, cleanup_errors)
}

/// Preserves the verification failure while reporting any cleanup failures.
pub fn finalize_verification(outcome: VerificationOutcome, cleanup_errors: Vec<String>) {
    let diagnostics = cleanup_diagnostics(&cleanup_errors);
    match outcome {
        Err(payload) => {
            eprintln!("cleanup errors: {diagnostics}");
            panic::resume_unwind(payload);
        }
        Ok(Err(error)) => {
            panic!(
                "real service PUT/GET/HEAD/LIST verification failed: {error}; cleanup errors: {diagnostics}"
            );
        }
        Ok(Ok(())) => assert!(cleanup_errors.is_empty(), "cleanup failed: {diagnostics}"),
    }
}

fn cleanup_diagnostics(errors: &[String]) -> String {
    if errors.is_empty() {
        "none".into()
    } else {
        errors.join("; ")
    }
}

fn panic_message(payload: &Box<dyn Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&'static str>()
                .map(|value| (*value).to_owned())
        })
        .unwrap_or_else(|| "non-string panic payload".into())
}
