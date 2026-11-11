// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cleanup for exact keys owned by a verification run.

use std::any::Any;
use std::error::Error;
use std::future::Future;
use std::panic;

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
) -> (VerificationOutcome, Vec<ObjectStoreError>)
where
    F: Future<Output = Result<(), Box<dyn Error>>>,
{
    run_with_exact_key_cleanup_using(owned, verification, |key| async move { store.delete(&key).await }).await
}

/// Test seam for exercising deletion failures without changing production
/// stores.
pub async fn run_with_exact_key_cleanup_using<F, D, DF>(
    owned: &[ObjectStorePath],
    verification: F,
    mut delete: D,
) -> (VerificationOutcome, Vec<ObjectStoreError>)
where
    F: Future<Output = Result<(), Box<dyn Error>>>,
    D: FnMut(ObjectStorePath) -> DF,
    DF: Future<Output = Result<(), ObjectStoreError>>,
{
    let outcome = panic::AssertUnwindSafe(verification).catch_unwind().await;
    let mut cleanup_errors = Vec::new();
    for key in owned {
        if let Err(error) = delete(key.clone()).await
            && !matches!(error, ObjectStoreError::NotFound { .. })
        {
            cleanup_errors.push(error);
        }
    }
    (outcome, cleanup_errors)
}

/// Preserves the verification failure while reporting any cleanup failures.
pub fn finalize_verification(outcome: VerificationOutcome, cleanup_errors: Vec<ObjectStoreError>) {
    match outcome {
        Err(payload) => {
            eprintln!("cleanup error count: {}", cleanup_errors.len());
            panic::resume_unwind(payload);
        }
        Ok(Err(error)) => {
            panic!(
                "real service PUT/GET/HEAD/LIST verification failed: {error}; cleanup error count: {}",
                cleanup_errors.len()
            );
        }
        Ok(Ok(())) => assert!(
            cleanup_errors.is_empty(),
            "cleanup failed for {} owned keys",
            cleanup_errors.len()
        ),
    }
}
