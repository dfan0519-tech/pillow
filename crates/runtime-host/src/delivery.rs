//! Bounded event delivery with explicit backpressure.
//!
//! Producers block when the queue is full until a consumer drains capacity or
//! the channel is closed. This replaces unbounded history cloning.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use keeper_agent_protocol::{
    RuntimeError, RuntimeEvent, RuntimeEventKind, WriteProposalResolvedPayload,
    WritePublicErrorCode, validate_payload,
};
use keeper_policy::WriteProposalState;

/// Default per-turn event queue capacity (host-owned delivery).
pub const DEFAULT_EVENT_QUEUE_CAPACITY: usize = 64;

#[derive(Debug)]
struct Inner {
    queue: VecDeque<RuntimeEvent>,
    capacity: usize,
    closed: bool,
    /// Host cancel requested: non-cancel terminals must not commit.
    cancelled: bool,
    /// After cancel, at most one safe `write_proposal_resolved` may still commit.
    post_cancel_write_resolution_committed: bool,
    /// True while a producer is blocked waiting for capacity (test/observability).
    producer_blocked: bool,
}

/// Narrow post-cancel write terminal: `cancelled`/`cancelled` or
/// `interrupted`/`interrupted` only. Shared by delivery commit arbitration and
/// the protocol guard.
pub(crate) fn is_safe_post_cancel_write_resolution(event: &RuntimeEvent) -> bool {
    if event.kind != RuntimeEventKind::WriteProposalResolved {
        return false;
    }
    let Ok(payload) = validate_payload(event.kind, &event.payload) else {
        return false;
    };
    let Ok(resolved) = serde_json::from_value::<WriteProposalResolvedPayload>(payload) else {
        return false;
    };
    matches!(
        (resolved.state, resolved.error_code),
        (
            WriteProposalState::Cancelled,
            Some(WritePublicErrorCode::Cancelled)
        ) | (
            WriteProposalState::Interrupted,
            Some(WritePublicErrorCode::Interrupted)
        )
    )
}

/// Whether this event may still enqueue after host cancel won arbitration.
///
/// Permits the host-forced `run_cancelled` terminal and at most one narrowly
/// validated `write_proposal_resolved` so the UI can refresh pending proposals
/// before the run closes. All other kinds remain rejected.
fn may_commit_after_cancel(inner: &Inner, event: &RuntimeEvent) -> bool {
    match event.kind {
        RuntimeEventKind::RunCancelled => true,
        RuntimeEventKind::WriteProposalResolved => {
            !inner.post_cancel_write_resolution_committed
                && is_safe_post_cancel_write_resolution(event)
        }
        _ => false,
    }
}

/// Bounded, multi-consumer-snapshot queue with blocking push backpressure.
#[derive(Debug)]
pub struct BoundedDelivery {
    inner: Mutex<Inner>,
    not_full: Condvar,
    not_empty: Condvar,
}

impl BoundedDelivery {
    pub fn new(capacity: usize) -> Arc<Self> {
        let capacity = capacity.max(1);
        Arc::new(Self {
            inner: Mutex::new(Inner {
                queue: VecDeque::with_capacity(capacity),
                capacity,
                closed: false,
                cancelled: false,
                post_cancel_write_resolution_committed: false,
                producer_blocked: false,
            }),
            not_full: Condvar::new(),
            not_empty: Condvar::new(),
        })
    }

    pub fn capacity(&self) -> usize {
        self.inner.lock().expect("delivery").capacity
    }

    /// Publish host cancel under the same lock that guards terminal commit.
    pub fn request_cancel(&self) {
        let mut guard = self.inner.lock().expect("delivery");
        guard.cancelled = true;
        // Wake a producer blocked on a full queue so it can re-arbitrate.
        self.not_full.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner.lock().expect("delivery").cancelled
    }

    /// Push an event, blocking when full until space is available, closed, or
    /// cancel wins the commit arbitration.
    ///
    /// After cancel, only `run_cancelled` and at most one
    /// `write_proposal_resolved` may commit. A previously validated but blocked
    /// `turn_completed` cannot win.
    pub fn push_blocking(&self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        let is_cancel_terminal = event.kind == RuntimeEventKind::RunCancelled;
        let mut guard = self.inner.lock().expect("delivery");
        loop {
            // Cancel beats a non-cancel terminal that was blocked on enqueue.
            if guard.cancelled && !may_commit_after_cancel(&guard, &event) {
                guard.producer_blocked = false;
                return Err(RuntimeError::Cancelled);
            }
            if guard.closed && !is_cancel_terminal {
                guard.producer_blocked = false;
                return Err(RuntimeError::ShutDown);
            }
            // Host cancel terminal may still wait for capacity after cancel.
            if guard.queue.len() < guard.capacity {
                break;
            }
            if guard.closed && is_cancel_terminal {
                // Closed and full: cannot record residual cancel terminal.
                guard.producer_blocked = false;
                return Err(RuntimeError::ShutDown);
            }
            guard.producer_blocked = true;
            guard = self.not_full.wait(guard).expect("delivery wait");
        }
        guard.producer_blocked = false;
        // Re-check after wait: cancel may have landed while blocked.
        if guard.cancelled && !may_commit_after_cancel(&guard, &event) {
            return Err(RuntimeError::Cancelled);
        }
        if guard.closed && !is_cancel_terminal {
            return Err(RuntimeError::ShutDown);
        }
        if guard.cancelled && event.kind == RuntimeEventKind::WriteProposalResolved {
            guard.post_cancel_write_resolution_committed = true;
        }
        guard.queue.push_back(event);
        self.not_empty.notify_one();
        Ok(())
    }

    /// Non-blocking push used when fail-fast backpressure is preferred.
    pub fn try_push(&self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        let mut guard = self.inner.lock().expect("delivery");
        if guard.cancelled && !may_commit_after_cancel(&guard, &event) {
            return Err(RuntimeError::Cancelled);
        }
        if guard.closed {
            return Err(RuntimeError::ShutDown);
        }
        if guard.queue.len() >= guard.capacity {
            return Err(RuntimeError::EventQueueFull {
                capacity: guard.capacity,
            });
        }
        if guard.cancelled && event.kind == RuntimeEventKind::WriteProposalResolved {
            guard.post_cancel_write_resolution_committed = true;
        }
        guard.queue.push_back(event);
        self.not_empty.notify_one();
        Ok(())
    }

    /// Pop the next event, waiting up to `timeout` when empty.
    pub fn pop_timeout(&self, timeout: Duration) -> Result<Option<RuntimeEvent>, RuntimeError> {
        let mut guard = self.inner.lock().expect("delivery");
        let mut waited = Duration::ZERO;
        while guard.queue.is_empty() && !guard.closed {
            let remaining = timeout.saturating_sub(waited);
            if remaining.is_zero() {
                return Ok(None);
            }
            let start = std::time::Instant::now();
            let (g, timeout_result) = self
                .not_empty
                .wait_timeout(guard, remaining)
                .expect("delivery wait");
            guard = g;
            waited += start.elapsed();
            if timeout_result.timed_out() && guard.queue.is_empty() {
                return Ok(None);
            }
        }
        if let Some(event) = guard.queue.pop_front() {
            self.not_full.notify_one();
            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    /// Non-blocking pop.
    pub fn try_pop(&self) -> Option<RuntimeEvent> {
        let mut guard = self.inner.lock().expect("delivery");
        let event = guard.queue.pop_front();
        if event.is_some() {
            self.not_full.notify_one();
        }
        event
    }

    /// Snapshot of currently buffered (not yet consumed) events.
    pub fn snapshot(&self) -> Vec<RuntimeEvent> {
        self.inner
            .lock()
            .expect("delivery")
            .queue
            .iter()
            .cloned()
            .collect()
    }

    /// Drain all currently buffered events without waiting.
    pub fn drain(&self) -> Vec<RuntimeEvent> {
        let mut guard = self.inner.lock().expect("delivery");
        let drained: Vec<_> = guard.queue.drain(..).collect();
        if !drained.is_empty() {
            self.not_full.notify_all();
        }
        drained
    }

    /// Collect remaining events until the producer closes and the queue empties.
    pub fn collect_remaining(&self) -> Vec<RuntimeEvent> {
        let mut out = Vec::new();
        loop {
            let mut guard = self.inner.lock().expect("delivery");
            while guard.queue.is_empty() && !guard.closed {
                guard = self.not_empty.wait(guard).expect("delivery wait");
            }
            if let Some(event) = guard.queue.pop_front() {
                self.not_full.notify_one();
                drop(guard);
                out.push(event);
                continue;
            }
            // closed and empty
            break;
        }
        out
    }

    pub fn close(&self) {
        let mut guard = self.inner.lock().expect("delivery");
        guard.closed = true;
        guard.producer_blocked = false;
        self.not_full.notify_all();
        self.not_empty.notify_all();
    }

    pub fn is_closed(&self) -> bool {
        self.inner.lock().expect("delivery").closed
    }

    pub fn len(&self) -> usize {
        self.inner.lock().expect("delivery").queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether a producer is currently blocked on a full queue.
    pub fn producer_blocked(&self) -> bool {
        self.inner.lock().expect("delivery").producer_blocked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use keeper_agent_protocol::{RunId, RuntimeEventKind};
    use std::thread;

    fn sample_event(seq: u64) -> RuntimeEvent {
        RuntimeEvent::new(
            RunId::new(),
            seq,
            RuntimeEventKind::Heartbeat,
            serde_json::json!({"tick": seq}),
            Utc::now(),
        )
        .unwrap()
    }

    #[test]
    fn push_blocks_when_full_until_pop() {
        let d = BoundedDelivery::new(1);
        d.push_blocking(sample_event(1)).unwrap();

        let d2 = Arc::clone(&d);
        let handle = thread::spawn(move || {
            d2.push_blocking(sample_event(2)).unwrap();
        });

        // Wait until producer is blocked on capacity.
        for _ in 0..100 {
            if d.producer_blocked() {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert!(d.producer_blocked(), "expected producer backpressure");
        assert_eq!(d.len(), 1);

        let first = d.try_pop().unwrap();
        assert_eq!(first.sequence, 1);
        handle.join().unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d.try_pop().unwrap().sequence, 2);
    }

    #[test]
    fn try_push_returns_queue_full() {
        let d = BoundedDelivery::new(1);
        d.try_push(sample_event(1)).unwrap();
        let err = d.try_push(sample_event(2)).unwrap_err();
        assert!(matches!(err, RuntimeError::EventQueueFull { capacity: 1 }));
    }

    fn write_resolved_event(seq: u64) -> RuntimeEvent {
        RuntimeEvent::new(
            RunId::new(),
            seq,
            RuntimeEventKind::WriteProposalResolved,
            serde_json::json!({
                "proposal_id": "wp-1",
                "path": "note.txt",
                "mode": "create",
                "state": "cancelled",
                "error_code": "cancelled"
            }),
            Utc::now(),
        )
        .unwrap()
    }

    fn cancel_terminal_event(seq: u64) -> RuntimeEvent {
        RuntimeEvent::new(
            RunId::new(),
            seq,
            RuntimeEventKind::RunCancelled,
            serde_json::json!({"reason": "host"}),
            Utc::now(),
        )
        .unwrap()
    }

    /// C3-F: after cancel, one write_proposal_resolved may still commit before
    /// the host-forced run_cancelled (payload pairing is enforced by the sink).
    #[test]
    fn after_cancel_one_write_resolution_then_run_cancelled_may_commit() {
        let d = BoundedDelivery::new(8);
        d.push_blocking(sample_event(1)).unwrap();
        d.request_cancel();
        d.push_blocking(write_resolved_event(2))
            .expect("first post-cancel write_proposal_resolved must commit");
        d.push_blocking(cancel_terminal_event(3))
            .expect("run_cancelled must still commit after safe resolution");
        let kinds: Vec<_> = d.drain().into_iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                RuntimeEventKind::Heartbeat,
                RuntimeEventKind::WriteProposalResolved,
                RuntimeEventKind::RunCancelled,
            ]
        );
    }

    #[test]
    fn after_cancel_second_write_resolution_and_non_cancel_terminals_are_rejected() {
        let d = BoundedDelivery::new(8);
        d.request_cancel();
        d.push_blocking(write_resolved_event(1)).unwrap();
        let dup = d.push_blocking(write_resolved_event(2)).unwrap_err();
        assert!(matches!(dup, RuntimeError::Cancelled));
        let completed = RuntimeEvent::new(
            RunId::new(),
            3,
            RuntimeEventKind::TurnCompleted,
            serde_json::json!({}),
            Utc::now(),
        )
        .unwrap();
        let blocked = d.push_blocking(completed).unwrap_err();
        assert!(matches!(blocked, RuntimeError::Cancelled));
        let kinds: Vec<_> = d.drain().into_iter().map(|e| e.kind).collect();
        assert_eq!(kinds, vec![RuntimeEventKind::WriteProposalResolved]);
    }
}
