//! Deterministic fixture-only gates at the adapter/SDK boundary.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

/// The exact adapter stage held pending by a test.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestStage {
    Open,
    Write,
    Flush,
    BeforePut,
    AfterPutBeforeResult,
    Abort,
}

#[derive(Default)]
struct Gate {
    stage: Option<TestStage>,
    reached: bool,
    observer: Option<Waker>,
    operation: Option<Waker>,
}

#[derive(Default)]
struct ControlState {
    gate: Mutex<Gate>,
    puts: AtomicUsize,
    accepted: AtomicUsize,
    aborts: AtomicUsize,
    fail_abort: AtomicBool,
}

/// Shared explicit test control; the default allows every stage immediately.
///
/// No timers, runtime tasks, or implicit retries are involved. A controlled
/// adapter must not be shared with unrelated operations while a gate is armed.
#[derive(Clone, Default)]
pub struct TestControl(Arc<ControlState>);

impl TestControl {
    /// Arms one stage before starting the operation; release an old gate first.
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

    /// Acknowledges arrival at the armed stage without advancing the operation.
    pub fn poll_reached(&self, cx: &mut Context<'_>) -> Poll<()> {
        let mut gate = self.0.gate.lock().expect("test gate lock");
        if gate.reached {
            Poll::Ready(())
        } else {
            gate.observer = Some(cx.waker().clone());
            Poll::Pending
        }
    }

    /// Makes the next polled abort fail once, retaining its session.
    pub fn fail_next_abort(&self) {
        self.0.fail_abort.store(true, Ordering::SeqCst);
    }

    /// Returns bytes acknowledged by the provider since the current gate was
    /// armed.
    pub fn accepted_bytes(&self) -> u64 {
        self.0.accepted.load(Ordering::SeqCst) as u64
    }

    /// Records accepted bytes independently of facade counters.
    pub(crate) fn record_write(&self, count: usize) {
        self.0.accepted.fetch_add(count, Ordering::SeqCst);
    }

    /// Returns the number of actual SDK PUT dispatches by this adapter.
    pub fn put_calls(&self) -> usize {
        self.0.puts.load(Ordering::SeqCst)
    }

    /// Returns the number of abort futures polled at least once.
    pub fn abort_calls(&self) -> usize {
        self.0.aborts.load(Ordering::SeqCst)
    }

    /// Parks at the selected boundary, acknowledging arrival exactly once.
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
