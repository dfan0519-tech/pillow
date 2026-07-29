//! Handle for a turn running on a host worker thread.

use std::sync::Arc;

use keeper_agent_protocol::{RunId, RuntimeError, RuntimeEvent};

use crate::worker::WorkerRecord;

use super::RuntimeHost;

/// Handle for a turn running on a host worker thread.
pub struct SpawnedTurn {
    pub run_id: RunId,
    pub(super) host: Arc<RuntimeHost>,
    pub(super) record: Arc<WorkerRecord>,
}

impl SpawnedTurn {
    pub fn cancel(&self) -> Result<(), RuntimeError> {
        self.host.cancel_turn(self.run_id)
    }

    pub fn is_finished(&self) -> bool {
        self.record.is_body_finished()
    }

    /// Snapshot of events currently buffered (not yet consumed).
    pub fn events_snapshot(&self) -> Vec<RuntimeEvent> {
        self.record.delivery.snapshot()
    }

    /// Delivery queue for streaming consumption / backpressure tests.
    pub fn delivery(&self) -> &Arc<crate::delivery::BoundedDelivery> {
        &self.record.delivery
    }

    pub fn join(self) -> Result<Vec<RuntimeEvent>, RuntimeError> {
        let mut events = Vec::new();
        let join_result = self.record.await_joined(&mut events);
        self.host.remove_worker_if_owner(self.run_id, &self.record);
        join_result?;
        Ok(events)
    }
}

impl Drop for SpawnedTurn {
    fn drop(&mut self) {
        if !self.record.is_joined() {
            // Request real adapter cancellation before waiting so a blocked
            // adapter (away from sink.emit) can unblock. Host cancel flag alone
            // is not sufficient for every provider. Exactly-one-terminal is
            // preserved by ProtocolGuardSink + delivery cancel arbitration.
            self.host
                .cancel_owned_worker_best_effort(self.run_id, &self.record);
            let mut discarded = Vec::new();
            let _ = self.record.await_joined(&mut discarded);
        }
        self.host.remove_worker_if_owner(self.run_id, &self.record);
    }
}
