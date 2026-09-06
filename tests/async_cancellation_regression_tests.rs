// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0 (the "License");
//    you may not use this file except in compliance with the License.
//    You may obtain a copy of the License at
//
//        http://www.apache.org/licenses/LICENSE-2.0
//
//    Unless required by applicable law or agreed to in writing, software
//    distributed under the License is distributed on an "AS IS" BASIS,
//    WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//    See the License for the specific language governing permissions and
//    limitations under the License.
// =============================================================================

#![cfg(feature = "async")]

mod common;

use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use common::AsyncMemoryFixture;
use common::run_controlled;
use qubit_fs_testkit::AsyncFileSystemContractSuite;
use qubit_fs_testkit::AsyncFileSystemFixture;

/// Every stage is acknowledged after multiple real provider suspensions.
#[test]
fn test_async_copy_cancellation_waits_for_each_stage() {
    let fixture = AsyncMemoryFixture::new();
    run_controlled(async {
        let mut suite = AsyncFileSystemContractSuite::new(&fixture);
        suite.assert_copy_cancellation().await;
        suite.finish().await;
    });
    assert!(fixture.is_empty(), "cancellation probes must be cleaned");
}

/// Dropping an in-flight assertion does not start asynchronous cleanup.
#[test]
fn test_async_copy_cancellation_drop_leaves_explicit_teardown_responsibility() {
    let fixture = AsyncMemoryFixture::new();
    let mut suite = AsyncFileSystemContractSuite::new(&fixture);
    let mut assertion = Box::pin(suite.assert_copy_cancellation());
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(
        assertion.as_mut().poll(&mut context),
        Poll::Pending
    ));
    drop(assertion);

    run_controlled(fixture.teardown()).expect("explicit fixture teardown must succeed");
    assert!(
        fixture.is_empty(),
        "explicit fixture teardown must reclaim data"
    );
}
