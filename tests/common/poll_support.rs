// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Runtime-neutral polling helpers for external asynchronous behavior tests.

use std::future::Future;
#[cfg(feature = "async")]
use std::pin::Pin;
use std::task::Context;
use std::task::Waker;

/// Polls a future that is expected to complete without scheduling work.
pub fn ready<F>(future: F) -> F::Output
where
    F: Future,
{
    let mut context = Context::from_waker(Waker::noop());
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut context) {
        std::task::Poll::Ready(output) => output,
        std::task::Poll::Pending => {
            panic!("test future should be immediately ready")
        }
    }
}

/// Verifies that one poll leaves a future pending without a runtime.
#[cfg(feature = "async")]
#[allow(dead_code)]
pub(crate) fn assert_pending<F>(mut future: Pin<&mut F>)
where
    F: Future + ?Sized,
{
    let mut context = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut context).is_pending());
}
