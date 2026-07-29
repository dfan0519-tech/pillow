//! Host-owned turn worker execution and join helpers.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use keeper_agent_protocol::{AgentRuntimeAdapter, RunId, RuntimeError, RuntimeEvent, SendTurn};

use crate::delivery::BoundedDelivery;
use crate::sink::{BoundedDeliverySink, ProtocolGuardSink};

/// Join-handle lifecycle for a host-owned worker.
///
/// `Starting` covers the window after registry insert and before the OS thread
/// handle is installed. Shutdown must wait through this state so it never
/// drops a live worker.
pub(crate) enum JoinSlot {
    Starting,
    Running(JoinHandle<Result<(), RuntimeError>>),
    Joined,
}

pub(crate) struct WorkerRecord {
    pub cancel: Arc<AtomicBool>,
    pub finished: Arc<AtomicBool>,
    pub delivery: Arc<BoundedDelivery>,
    pub join: std::sync::Mutex<JoinSlot>,
}

pub(crate) fn run_turn_worker(
    adapter: &Arc<dyn AgentRuntimeAdapter>,
    request: SendTurn,
    run_id: RunId,
    cancel: &AtomicBool,
    delivery: &Arc<BoundedDelivery>,
) -> Result<(), RuntimeError> {
    let mut sink = BoundedDeliverySink::new(Arc::clone(delivery));
    let mut guarded = ProtocolGuardSink::with_cancel(run_id, &mut sink, cancel);

    // Cancel may have been requested before send_turn started.
    if cancel.load(Ordering::SeqCst) {
        guarded.force_run_cancelled()?;
        return guarded.finish();
    }

    match adapter.send_turn(request, &mut guarded) {
        Ok(()) => {}
        Err(RuntimeError::Cancelled) => {}
        Err(e) => {
            if cancel.load(Ordering::SeqCst) && !guarded.terminal_seen() {
                let _ = guarded.force_run_cancelled();
                return guarded.finish();
            }
            return Err(e);
        }
    }

    if cancel.load(Ordering::SeqCst) && !guarded.terminal_seen() {
        guarded.force_run_cancelled()?;
    }
    guarded.finish()
}

/// Drain delivery capacity while waiting for a worker so a full bounded queue
/// cannot deadlock `join` / Drop when no external consumer is present.
pub(crate) fn join_handle_draining(
    delivery: &BoundedDelivery,
    handle: JoinHandle<Result<(), RuntimeError>>,
    out: &mut Vec<RuntimeEvent>,
) -> Result<(), RuntimeError> {
    loop {
        out.extend(delivery.drain());
        if handle.is_finished() {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    let join_result = handle.join().map_err(|_| RuntimeError::Internal {
        message: "turn worker panicked".into(),
    })?;
    out.extend(delivery.drain());
    join_result
}

/// Wait for a worker's join slot to resolve, draining the queue throughout.
pub(crate) fn await_worker_slot(
    record: &WorkerRecord,
    events: &mut Vec<RuntimeEvent>,
) -> Result<(), RuntimeError> {
    loop {
        let handle = {
            let mut slot = record.join.lock().expect("join");
            match std::mem::replace(&mut *slot, JoinSlot::Joined) {
                JoinSlot::Running(h) => Some(h),
                JoinSlot::Starting => {
                    *slot = JoinSlot::Starting;
                    None
                }
                JoinSlot::Joined => None,
            }
        };
        if let Some(handle) = handle {
            return join_handle_draining(&record.delivery, handle, events);
        }
        if record.finished.load(Ordering::SeqCst) {
            events.extend(record.delivery.drain());
            return Ok(());
        }
        events.extend(record.delivery.drain());
        thread::sleep(Duration::from_millis(1));
    }
}
