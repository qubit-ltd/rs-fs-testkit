// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Deterministic fixture-only gates at the adapter/SDK boundary.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;

use crate::TestStage;
use crate::test_control::internal::ControlState;

mod internal;

/// Shared explicit test control; the default allows every stage immediately.
///
/// No timers, runtime tasks, or implicit retries are involved. A controlled
/// adapter must not be shared with unrelated operations while a gate is armed.
///
/// # Examples
///
/// ```
/// use qubit_fs_s3_contract::TestControl;
///
/// let control = TestControl::default();
/// assert_eq!(control.put_calls(), 0);
/// ```
#[derive(Clone, Default)]
pub struct TestControl(
    /// Shared gate and observation state used by all control handles.
    Arc<ControlState>,
);

impl TestControl {
    /// Returns bytes acknowledged by the provider since the current gate was
    /// armed.
    ///
    /// # Returns
    ///
    /// The number of bytes acknowledged since the most recent call to `arm`.
    #[must_use]
    pub fn accepted_bytes(&self) -> u64 {
        self.0.accepted.load(Ordering::SeqCst) as u64
    }

    /// Returns the number of actual SDK PUT dispatches by this adapter.
    ///
    /// # Returns
    ///
    /// The number of PUT futures dispatched by the adapter.
    #[must_use]
    pub fn put_calls(&self) -> usize {
        self.0.puts.load(Ordering::SeqCst)
    }

    /// Returns the number of abort futures polled at least once.
    ///
    /// # Returns
    ///
    /// The number of abort futures polled by the adapter.
    #[must_use]
    pub fn abort_calls(&self) -> usize {
        self.0.aborts.load(Ordering::SeqCst)
    }

    /// Acknowledges arrival at the armed stage without advancing the operation.
    ///
    /// # Parameters
    ///
    /// * `cx` — task context used to register the observer's waker while the
    ///   armed stage has not been reached.
    ///
    /// # Returns
    ///
    /// `Poll::Ready(())` when the stage has been reached; otherwise,
    /// `Poll::Pending` after registering the observer's waker.
    pub fn poll_reached(&self, cx: &mut Context<'_>) -> Poll<()> {
        let mut gate = self.0.gate.lock().expect("test gate lock");
        if gate.reached {
            Poll::Ready(())
        } else {
            gate.observer = Some(cx.waker().clone());
            Poll::Pending
        }
    }

    /// Arms one stage before starting the operation; release an old gate first.
    ///
    /// # Parameters
    ///
    /// * `stage` — operation boundary at which the adapter should pause.
    pub fn arm(&self, stage: TestStage) {
        let mut gate = self.0.gate.lock().expect("test gate lock");
        assert!(gate.stage.is_none(), "release the previous test gate before rearming");
        self.0.accepted.store(0, Ordering::SeqCst);
        gate.stage = Some(stage);
        gate.reached = false;
    }

    /// Releases a pending operation and removes the armed stage.
    pub fn release(&self) {
        let waker = {
            let mut gate = self.0.gate.lock().expect("test gate lock");
            gate.stage = None;
            gate.operation.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// Makes the next polled abort fail once, retaining its session.
    pub fn fail_next_abort(&self) {
        self.0.fail_abort.store(true, Ordering::SeqCst);
    }

    /// Records accepted bytes independently of facade counters.
    ///
    /// # Parameters
    ///
    /// - `count`: Number of bytes accepted by the adapter.
    pub(crate) fn record_write(&self, count: usize) {
        self.0.accepted.fetch_add(count, Ordering::SeqCst);
    }

    /// Parks at the selected boundary, acknowledging arrival exactly once.
    ///
    /// # Parameters
    ///
    /// - `stage`: Lifecycle boundary controlled by this gate.
    /// - `cx`: Task context used to register operation and observer wakers.
    ///
    /// # Returns
    ///
    /// `Pending` while the selected stage remains armed; otherwise `Ready(())`.
    pub(crate) fn poll_gate(&self, stage: TestStage, cx: &mut Context<'_>) -> Poll<()> {
        let observer = {
            let mut gate = self.0.gate.lock().expect("test gate lock");
            if gate.stage != Some(stage) {
                return Poll::Ready(());
            }
            gate.reached = true;
            gate.operation = Some(cx.waker().clone());
            gate.observer.take()
        };
        if let Some(observer) = observer {
            observer.wake();
        }
        Poll::Pending
    }

    /// Waits only for an explicit release by the test or probe.
    ///
    /// # Parameters
    ///
    /// - `stage`: Lifecycle boundary to wait at until released.
    pub(crate) async fn wait(&self, stage: TestStage) {
        std::future::poll_fn(|cx| self.poll_gate(stage, cx)).await;
    }

    /// Records dispatch immediately before the SDK PUT future is first polled.
    pub(crate) fn record_put(&self) {
        self.0.puts.fetch_add(1, Ordering::SeqCst);
    }

    /// Records one abort attempt and consumes a configured one-shot failure.
    pub(crate) fn begin_abort(&self) -> bool {
        self.0.aborts.fetch_add(1, Ordering::SeqCst);
        self.0.fail_abort.swap(false, Ordering::SeqCst)
    }
}
