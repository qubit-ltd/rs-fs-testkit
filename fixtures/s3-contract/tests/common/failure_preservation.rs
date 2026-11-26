// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Preserves recovery verification failures while independently capturing
//! teardown outcomes.

use std::any::Any;
use std::error::Error;
use std::future::Future;
use std::panic;

use futures_util::FutureExt;

/// Runs verification and teardown in order, retaining verification panic as the
/// primary failure.
pub async fn verify_then_cleanup<V, C, E, H>(verification: V, cleanup: C, mut report: H)
where
    V: Future<Output = ()>,
    C: Future<Output = Result<(), E>>,
    E: Error,
    H: FnMut(String),
{
    let verification = panic::AssertUnwindSafe(verification).catch_unwind().await;
    let cleanup = panic::AssertUnwindSafe(cleanup).catch_unwind().await;

    match verification {
        Err(primary) => {
            match cleanup {
                Ok(Ok(())) => report("fixture teardown completed".into()),
                Ok(Err(error)) => report(format!("fixture teardown failed: {error}")),
                Err(cleanup_panic) => report(format!("fixture teardown panicked: {}", panic_message(&cleanup_panic))),
            }
            panic::resume_unwind(primary);
        }
        Ok(()) => match cleanup {
            Ok(Ok(())) => {}
            Ok(Err(error)) => panic!("fixture teardown failed: {error}"),
            Err(cleanup_panic) => {
                report(format!("fixture teardown panicked: {}", panic_message(&cleanup_panic)));
                panic::resume_unwind(cleanup_panic);
            }
        },
    }
}

fn panic_message(payload: &Box<dyn Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&'static str>().map(|value| (*value).to_owned()))
        .unwrap_or_else(|| "non-string panic payload".into())
}
