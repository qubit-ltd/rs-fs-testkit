// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Independently observed suspension and byte acceptance for write probes.

use std::task::Context;
use std::task::Poll;

use crate::qubit_fs_testkit::AsyncWriteCancellationStage;
use crate::qubit_fs_testkit::FixtureError;
use crate::qubit_fs_testkit::FixtureResult;

/// One provider-side gate with deliberate wake-driven suspensions.
#[must_use]
pub(crate) struct WriteGate {
    stage: Option<AsyncWriteCancellationStage>,
    remaining: usize,
    reached: bool,
    pub(crate) accepted: u64,
    pub(crate) observed: Vec<AsyncWriteCancellationStage>,
}

impl WriteGate {
    /// Creates a disarmed gate with no accepted bytes or observed stages.
    pub(crate) fn new() -> Self {
        Self {
            stage: None,
            remaining: 0,
            reached: false,
            accepted: 0,
            observed: Vec::new(),
        }
    }

    /// Reports whether a stage is currently armed.
    #[must_use]
    #[inline]
    pub(crate) fn is_armed(&self) -> bool {
        self.stage.is_some()
    }

    /// Arms the selected stage after two pending polls and resets its
    /// observations.
    pub(crate) fn arm(&mut self, stage: AsyncWriteCancellationStage) {
        self.stage = Some(stage);
        self.remaining = 2;
        self.reached = false;
        self.accepted = 0;
    }

    /// Suspends polls for the selected stage and records its first reached
    /// poll.
    pub(crate) fn poll(&mut self, stage: AsyncWriteCancellationStage, context: &Context<'_>) -> Poll<()> {
        if self.stage != Some(stage) {
            return Poll::Ready(());
        }
        if self.remaining > 0 {
            self.remaining -= 1;
        } else if !self.reached {
            self.reached = true;
            self.observed.push(stage);
        }
        context.waker().wake_by_ref();
        Poll::Pending
    }

    /// Reports whether the armed stage has been reached, or an early-disarm
    /// error.
    pub(crate) fn poll_reached(&self) -> Poll<FixtureResult<()>> {
        if self.stage.is_none() {
            return Poll::Ready(Err(FixtureError::new("write gate disarmed before acknowledgement")));
        }
        if self.reached {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }

    /// Disarms the gate so subsequent polls are immediately ready.
    pub(crate) fn disarm(&mut self) {
        self.stage = None;
    }
}
