// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Write cancellation instrumentation independent of the tested facade.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Context;
use std::task::Poll;

use super::shared_model::Entry;
use super::write_gate::WriteGate;
use crate::qubit_fs_testkit::AsyncWriteFixtureCase;
use crate::qubit_fs_testkit::FixtureError;
use crate::qubit_fs_testkit::FixtureFuture;
use crate::qubit_fs_testkit::FixtureResult;
use crate::qubit_fs_testkit::WriteCancellationProbe;

pub(crate) struct AsyncMemoryWriteCancellationProbe {
    pub(crate) case: AsyncWriteFixtureCase,
    pub(crate) entries: Arc<Mutex<HashMap<String, Entry>>>,
    pub(crate) gate: Arc<Mutex<WriteGate>>,
    pub(crate) fault: Option<ProbeFault>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProbeFault {
    Observe,
    Acknowledge,
    AcceptedBytes,
    Disarm,
}

impl WriteCancellationProbe for AsyncMemoryWriteCancellationProbe {
    fn case(&self) -> &AsyncWriteFixtureCase {
        &self.case
    }
    fn poll_reached(&self, _: &mut Context<'_>) -> Poll<FixtureResult<()>> {
        if self.fault == Some(ProbeFault::Acknowledge) {
            return Poll::Ready(Err(FixtureError::new("injected write acknowledgement failure")));
        }
        self.gate.lock().expect("write gate lock").poll_reached()
    }
    fn accepted_bytes(&self) -> FixtureResult<u64> {
        if self.fault == Some(ProbeFault::AcceptedBytes) {
            return Err(FixtureError::new("injected accepted-byte observation failure"));
        }
        Ok(self.gate.lock().expect("write gate lock").accepted)
    }
    fn observe_target(&self) -> FixtureFuture<'_, Option<Vec<u8>>> {
        Box::pin(async move {
            if self.fault == Some(ProbeFault::Observe) {
                return Err(FixtureError::new("injected write target observation failure"));
            }
            let mut suspended = false;
            std::future::poll_fn(|context| {
                if suspended {
                    return Poll::Ready(());
                }
                suspended = true;
                context.waker().wake_by_ref();
                Poll::Pending
            })
            .await;
            match self
                .entries
                .lock()
                .expect("memory observation lock")
                .get(self.case.path().as_str())
            {
                Some(Entry::File(bytes)) => Ok(Some(bytes.clone())),
                None => Ok(None),
                Some(_) => Err(FixtureError::new("write target is not a file")),
            }
        })
    }
    fn disarm(&self) -> FixtureResult<()> {
        self.gate.lock().expect("write gate lock").disarm();
        if self.fault == Some(ProbeFault::Disarm) {
            return Err(FixtureError::new("injected write probe disarm failure"));
        }
        Ok(())
    }
}
