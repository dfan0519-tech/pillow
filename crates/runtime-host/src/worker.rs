//! Host-owned turn worker: explicit lifecycle state machine and join helpers.
//!
//! Lifecycle is linearizable under [`WorkerRecord`]'s mutex + condvar:
//!
//! `Starting -> Running -> Joining -> Joined`
//!
//! Exactly one waiter claims the OS join handle (`Joining`); other waiters
//! block until the phase is truly `Joined`. Cancel intent is published through
//! the same record (and into delivery so terminal commit is atomic with cancel).
//!
//! Async concurrency slots are owned as an RAII [`TurnSlotPermit`] on the
//! record: acquire once under the host workers lock, drop exactly once on
//! failure cleanup or worker completion (never via scattered manual releases).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use keeper_agent_protocol::{AgentRuntimeAdapter, RunId, RuntimeError, RuntimeEvent, SendTurn};

use crate::delivery::BoundedDelivery;
use crate::sink::{BoundedDeliverySink, ProtocolGuardSink};

/// Single-owner concurrency permit for one turn slot.
///
/// Increments the shared counter exactly once on successful acquire and
/// decrements exactly once on [`Drop`]. No manual release API — drop (or
/// `mem::forget` is intentionally not used) is the only path.
pub(crate) struct TurnSlotPermit {
    counter: Arc<AtomicU32>,
}

impl TurnSlotPermit {
    /// CAS-increment `counter` if below `max`.
    pub fn try_acquire(counter: &Arc<AtomicU32>, max: u32) -> Result<Self, RuntimeError> {
        loop {
            let current = counter.load(Ordering::SeqCst);
            if current >= max {
                return Err(RuntimeError::ConcurrencyLimit { max });
            }
            if counter
                .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return Ok(Self {
                    counter: Arc::clone(counter),
                });
            }
        }
    }
}

impl Drop for TurnSlotPermit {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Linearizes synchronous `send_turn` admission with shutdown.
///
/// Sync turns share the concurrency counter with async workers but are not
/// registered as OS worker records. This gate is the single-owner lifecycle:
/// admit increments `in_flight` under the mutex; release decrements and
/// notifies; shutdown closes the gate (no new admits) then waits on the
/// condition variable until every admitted turn has released.
///
/// No polling: waiters block on [`Condvar`] until a release or close wakes them.
#[derive(Debug)]
pub(crate) struct SyncTurnLifecycle {
    state: Mutex<SyncTurnState>,
    cv: Condvar,
}

#[derive(Debug)]
struct SyncTurnState {
    /// Admitted sync turns that have not yet released their admission.
    in_flight: u32,
    /// When true, no further admissions succeed (`RuntimeError::ShutDown`).
    closed: bool,
}

/// RAII admission for one synchronous turn. Drop releases the lifecycle slot.
pub(crate) struct SyncTurnAdmission<'a> {
    life: &'a SyncTurnLifecycle,
}

impl SyncTurnLifecycle {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(SyncTurnState {
                in_flight: 0,
                closed: false,
            }),
            cv: Condvar::new(),
        }
    }

    /// Admit one sync turn, or reject if shutdown has closed the gate.
    pub fn try_admit(&self) -> Result<SyncTurnAdmission<'_>, RuntimeError> {
        let mut g = self.state.lock().expect("sync turn lifecycle");
        if g.closed {
            return Err(RuntimeError::ShutDown);
        }
        g.in_flight = g.in_flight.saturating_add(1);
        Ok(SyncTurnAdmission { life: self })
    }

    /// Close the gate so no new sync turns are admitted.
    pub fn close(&self) {
        let mut g = self.state.lock().expect("sync turn lifecycle");
        g.closed = true;
        // Wake any waiter that might be in `wait_until_idle` after a prior close.
        self.cv.notify_all();
    }

    /// Block until every admitted sync turn has released (condition-based).
    pub fn wait_until_idle(&self) {
        let mut g = self.state.lock().expect("sync turn lifecycle");
        while g.in_flight > 0 {
            g = self.cv.wait(g).expect("sync turn lifecycle cv");
        }
    }

    /// Admitted sync turns still holding the lifecycle counter (tests / idle).
    pub fn in_flight(&self) -> u32 {
        self.state.lock().expect("sync turn lifecycle").in_flight
    }
}

impl Drop for SyncTurnAdmission<'_> {
    fn drop(&mut self) {
        let mut g = self.life.state.lock().expect("sync turn lifecycle");
        g.in_flight = g.in_flight.saturating_sub(1);
        self.life.cv.notify_all();
    }
}

/// Linearizable host worker lifecycle.
#[derive(Debug)]
pub(crate) enum WorkerPhase {
    /// Registered in the host map; may still be arming or spawning the OS thread.
    Starting,
    /// OS thread is live; this record owns the join handle.
    Running(JoinHandle<Result<(), RuntimeError>>),
    /// Exactly one waiter holds the handle and is performing the OS join.
    Joining,
    /// OS join finished; no residual host work for this run.
    Joined,
}

struct WorkerInner {
    phase: WorkerPhase,
    /// Worker thread body returned (handle may still be in `Joining`).
    body_finished: bool,
}

/// Per-run worker record: sole authority for phase, cancel publish, join, and
/// the async turn-slot permit.
pub(crate) struct WorkerRecord {
    pub cancel: Arc<AtomicBool>,
    pub delivery: Arc<BoundedDelivery>,
    /// Async slot permit. Taken exactly once (failure cleanup or completion).
    slot: Mutex<Option<TurnSlotPermit>>,
    inner: Mutex<WorkerInner>,
    cv: Condvar,
}

impl WorkerRecord {
    pub fn new(capacity: usize, permit: TurnSlotPermit) -> Arc<Self> {
        Arc::new(Self {
            cancel: Arc::new(AtomicBool::new(false)),
            delivery: BoundedDelivery::new(capacity),
            slot: Mutex::new(Some(permit)),
            inner: Mutex::new(WorkerInner {
                phase: WorkerPhase::Starting,
                body_finished: false,
            }),
            cv: Condvar::new(),
        })
    }

    /// Whether this record still owns its async turn-slot permit.
    pub fn has_slot_permit(&self) -> bool {
        self.slot.lock().expect("slot").is_some()
    }

    /// Drop the slot permit exactly once. Further calls are no-ops.
    ///
    /// Used by the worker body (free concurrency early) and as a safety net
    /// when publishing `Joined`.
    pub fn release_slot_permit(&self) {
        drop(self.slot.lock().expect("slot").take());
    }

    /// Publish cancel so ingress and terminal commit observe it.
    pub fn request_cancel(&self) {
        // Delivery cancel is authoritative for enqueue/commit arbitration.
        self.delivery.request_cancel();
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn is_body_finished(&self) -> bool {
        self.inner.lock().expect("worker").body_finished
    }

    pub fn is_joined(&self) -> bool {
        matches!(
            self.inner.lock().expect("worker").phase,
            WorkerPhase::Joined
        )
    }

    pub fn mark_body_finished(&self) {
        let mut g = self.inner.lock().expect("worker");
        g.body_finished = true;
        self.cv.notify_all();
    }

    /// Transition `Starting -> Running` after the OS thread is spawned.
    pub fn install_running(&self, handle: JoinHandle<Result<(), RuntimeError>>) {
        let mut g = self.inner.lock().expect("worker");
        match g.phase {
            WorkerPhase::Starting => {
                g.phase = WorkerPhase::Running(handle);
                self.cv.notify_all();
            }
            // Failure / shutdown races: do not leak the handle.
            _ => {
                drop(g);
                let _ = handle.join();
            }
        }
    }

    /// Fail a spawn that never obtained a live handle (`Starting -> Joined`).
    ///
    /// Releases the async slot permit **before** publishing `Joined` so a
    /// concurrent `join_all` waiter cannot observe Joined / an empty registry
    /// while the permit is still alive.
    pub fn abandon_starting(&self) {
        // Permit first — critical ordering vs shutdown's in_flight check.
        self.release_slot_permit();
        let mut g = self.inner.lock().expect("worker");
        if matches!(g.phase, WorkerPhase::Starting) {
            g.phase = WorkerPhase::Joined;
            g.body_finished = true;
            self.cv.notify_all();
        }
    }

    /// Wait until the worker is truly `Joined`.
    ///
    /// Exactly one caller transitions `Running -> Joining` and performs the OS
    /// join; concurrent waiters block on `Joining` until `Joined`.
    pub fn await_joined(&self, events: &mut Vec<RuntimeEvent>) -> Result<(), RuntimeError> {
        let handle = {
            let mut g = self.inner.lock().expect("worker");
            loop {
                match g.phase {
                    WorkerPhase::Running(_) => {
                        let prev = std::mem::replace(&mut g.phase, WorkerPhase::Joining);
                        let WorkerPhase::Running(h) = prev else {
                            unreachable!("phase was Running");
                        };
                        break Some(h);
                    }
                    WorkerPhase::Joined => break None,
                    WorkerPhase::Starting | WorkerPhase::Joining => {
                        g = self.cv.wait(g).expect("worker cv");
                    }
                }
            }
        };

        match handle {
            Some(h) => {
                let result = join_handle_draining(&self.delivery, h, events);
                // Safety net: body should already have dropped the permit; take
                // is idempotent. Always drop before publishing Joined.
                self.release_slot_permit();
                {
                    let mut g = self.inner.lock().expect("worker");
                    g.phase = WorkerPhase::Joined;
                    g.body_finished = true;
                }
                self.cv.notify_all();
                result
            }
            None => {
                // Already Joined (abandon or prior waiter); permit must be gone.
                events.extend(self.delivery.drain());
                Ok(())
            }
        }
    }
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
    if cancel.load(Ordering::SeqCst) || delivery.is_cancelled() {
        guarded.force_run_cancelled()?;
        return guarded.finish();
    }

    match adapter.send_turn(request, &mut guarded) {
        Ok(()) => {}
        Err(RuntimeError::Cancelled) => {}
        Err(e) => {
            if (cancel.load(Ordering::SeqCst) || delivery.is_cancelled())
                && !guarded.terminal_seen()
            {
                let _ = guarded.force_run_cancelled();
                return guarded.finish();
            }
            return Err(e);
        }
    }

    if (cancel.load(Ordering::SeqCst) || delivery.is_cancelled()) && !guarded.terminal_seen() {
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
