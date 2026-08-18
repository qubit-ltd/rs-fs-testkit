// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================

mod common;

#[cfg(feature = "async")]
use std::future::Future;
#[cfg(feature = "async")]
use std::task::Context;
#[cfg(feature = "async")]
use std::task::Poll;
#[cfg(feature = "async")]
use std::task::Waker;

#[cfg(feature = "async")]
use common::AsyncMemoryFixture;
use common::MemoryFixture;
#[cfg(feature = "async")]
use qubit_fs_testkit::register_async_file_system_contract_tests;
use qubit_fs_testkit::register_file_system_contract_tests;

/// Drives a ready-only memory fixture future used by macro registration tests.
#[cfg(feature = "async")]
fn run_ready(future: impl Future<Output = ()>) {
    let mut future = Box::pin(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(
        future.as_mut().poll(&mut context),
        Poll::Ready(())
    ));
}

register_file_system_contract_tests! {
    module: registered_sync_contracts,
    fixture: super::MemoryFixture::new,
}

#[cfg(feature = "async")]
register_async_file_system_contract_tests! {
    module: registered_async_contracts,
    fixture: super::AsyncMemoryFixture::new,
    runner: super::run_ready,
}
