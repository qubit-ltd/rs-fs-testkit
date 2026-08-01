// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================

mod common;

use std::future::Future;
use std::task::{
    Context,
    Poll,
    Waker,
};

use common::{
    AsyncMemoryFixture,
    MemoryFixture,
};

/// Drives a ready-only memory fixture future used by macro registration tests.
fn run_ready(future: impl Future<Output = ()>) {
    let mut future = Box::pin(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(
        future.as_mut().poll(&mut context),
        Poll::Ready(())
    ));
}

qubit_fs_testkit::register_file_system_contract_tests! {
    module: registered_sync_contracts,
    fixture: super::MemoryFixture::new,
}

qubit_fs_testkit::register_async_file_system_contract_tests! {
    module: registered_async_contracts,
    fixture: super::AsyncMemoryFixture::new,
    runner: super::run_ready,
}
