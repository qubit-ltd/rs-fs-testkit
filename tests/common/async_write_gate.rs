// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Provider-observed write gates used by cancellation contract self-tests.

use std::sync::Arc;
use std::sync::Mutex;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_fs_testkit::AsyncWriteCancellationStage;
use qubit_fs_testkit::AsyncWriteFixtureCase;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::WriteCancellationProbe;

/// Shared stage gate used by the asynchronous cancellation self-test.
#[cfg(feature = "async")]
pub(super) struct WriteGate {
    stage: AsyncWriteCancellationStage,
    pending_before_reached: u8,
    target_reached: bool,
    pub(super) armed: bool,
    pub(super) fail_acknowledgement: bool,
    pub(super) fail_disarm: bool,
    pub(super) prepared: Vec<AsyncWriteCancellationStage>,
    waker: Option<Waker>,
}

#[cfg(feature = "async")]
impl WriteGate {
    /// Creates a disarmed gate with no retained caller waker.
    pub(super) fn new() -> Self {
        Self {
            stage: AsyncWriteCancellationStage::Open,
            pending_before_reached: 0,
            target_reached: false,
            armed: false,
            fail_acknowledgement: false,
            fail_disarm: false,
            prepared: Vec::new(),
            waker: None,
        }
    }

    /// Arms a stage with two deliberate pre-acknowledgement suspensions.
    pub(super) fn arm(&mut self, stage: AsyncWriteCancellationStage) {
        self.prepared.push(stage);
        self.stage = stage;
        self.pending_before_reached = 2;
        self.target_reached = false;
        self.armed = true;
        self.waker = None;
    }

    /// Suspends the provider operation until the target stage is reached.
    pub(super) fn poll_stage(&mut self, stage: AsyncWriteCancellationStage, context: &Context<'_>) -> Poll<()> {
        if !self.armed || self.stage != stage {
            return Poll::Ready(());
        }
        self.waker = Some(context.waker().clone());
        if self.pending_before_reached != 0 {
            self.pending_before_reached -= 1;
            context.waker().wake_by_ref();
            return Poll::Pending;
        }
        if !self.target_reached {
            self.target_reached = true;
            context.waker().wake_by_ref();
        }
        Poll::Pending
    }

    /// Polls stage acknowledgement using the caller's real waker.
    fn poll_reached(&mut self, context: &Context<'_>) -> Poll<FixtureResult<()>> {
        if self.target_reached {
            if self.fail_acknowledgement {
                return Poll::Ready(Err(FixtureError::new("injected write acknowledgement failure")));
            }
            return Poll::Ready(Ok(()));
        }
        if !self.armed {
            return Poll::Ready(Err(FixtureError::new(
                "asynchronous write cancellation gate was disarmed before acknowledgement",
            )));
        }
        self.waker = Some(context.waker().clone());
        Poll::Pending
    }

    /// Releases the gate and wakes the operation owner, without doing I/O.
    fn disarm(&mut self) -> FixtureResult<()> {
        self.armed = false;
        self.target_reached = false;
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
        if self.fail_disarm {
            return Err(FixtureError::new("injected write disarm failure"));
        }
        Ok(())
    }
}

/// Provider-owned cancellation probe backed by the memory fixture gate.
#[cfg(feature = "async")]
pub(super) struct AsyncMemoryWriteCancellationProbe {
    pub(super) case: AsyncWriteFixtureCase,
    pub(super) gate: Arc<Mutex<WriteGate>>,
}

#[cfg(feature = "async")]
impl WriteCancellationProbe for AsyncMemoryWriteCancellationProbe {
    /// Returns the isolated request controlled by this probe.
    fn case(&self) -> &AsyncWriteFixtureCase {
        &self.case
    }

    /// Reports only the provider-observed target stage.
    fn poll_reached(&self, context: &mut Context<'_>) -> Poll<FixtureResult<()>> {
        self.gate
            .lock()
            .expect("async write gate lock must succeed")
            .poll_reached(context)
    }

    /// Releases the gate without starting an asynchronous operation.
    fn disarm(&self) -> FixtureResult<()> {
        self.gate.lock().expect("async write gate lock must succeed").disarm()
    }
}

#[cfg(feature = "async")]
impl Drop for AsyncMemoryWriteCancellationProbe {
    /// Ensures a dropped probe cannot leave a provider gate armed.
    fn drop(&mut self) {
        let _ = self.disarm();
    }
}
