// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow source-test-pair
//! Runtime-neutral panic capture for asynchronous contract execution.

use std::any::Any;
use std::future::Future;
use std::future::poll_fn;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::task::Poll;

/// Polls a future while converting an assertion panic into a result.
pub(crate) async fn catch_unwind_future<F>(future: F) -> Result<F::Output, Box<dyn Any + Send>>
where
    F: Future,
{
    let mut future = Box::pin(future);
    poll_fn(
        move |context| match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(context))) {
            Ok(Poll::Ready(output)) => Poll::Ready(Ok(output)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(payload) => Poll::Ready(Err(payload)),
        },
    )
    .await
}
