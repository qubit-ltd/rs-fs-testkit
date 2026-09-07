// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Write cancellation instrumentation independent of the tested facade.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Context;
use std::task::Poll;

use qubit_fs_testkit::AsyncWriteFixtureCase;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureFuture;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::WriteCancellationProbe;

use super::shared_model::Entry;
use super::write_gate::WriteGate;

pub(crate) struct AsyncMemoryWriteCancellationProbe {
    pub(crate) case: AsyncWriteFixtureCase,
    pub(crate) entries: Arc<Mutex<HashMap<String, Entry>>>,
    pub(crate) gate: Arc<Mutex<WriteGate>>,
}

impl WriteCancellationProbe for AsyncMemoryWriteCancellationProbe {
    fn case(&self) -> &AsyncWriteFixtureCase {
        &self.case
    }
    fn poll_reached(&self, _: &mut Context<'_>) -> Poll<FixtureResult<()>> {
        self.gate.lock().expect("write gate lock").poll_reached()
    }
    fn accepted_bytes(&self) -> FixtureResult<u64> {
        Ok(self.gate.lock().expect("write gate lock").accepted)
    }
    fn observe_target(&self) -> FixtureFuture<'_, Option<Vec<u8>>> {
        Box::pin(async move {
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
        Ok(())
    }
}
