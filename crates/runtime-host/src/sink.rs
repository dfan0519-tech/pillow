//! Event sinks used by the host and tests.

use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Utc;
use keeper_agent_protocol::{
    EventSink, RunCancelledPayload, RunId, RuntimeError, RuntimeEvent, RuntimeEventKind,
    validate_payload,
};

use crate::delivery::BoundedDelivery;

/// Collects events into a vector (tests and in-process capture).
#[derive(Debug, Default)]
pub struct CollectingSink {
    pub events: Vec<RuntimeEvent>,
}

impl EventSink for CollectingSink {
    fn emit(&mut self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        self.events.push(event);
        Ok(())
    }
}

/// Rejects any event after the first terminal kind.
pub struct TerminalGuardSink<'a> {
    inner: &'a mut dyn EventSink,
    terminal_seen: bool,
}

impl<'a> TerminalGuardSink<'a> {
    pub fn new(inner: &'a mut dyn EventSink) -> Self {
        Self {
            inner,
            terminal_seen: false,
        }
    }

    pub fn terminal_seen(&self) -> bool {
        self.terminal_seen
    }
}

impl EventSink for TerminalGuardSink<'_> {
    fn emit(&mut self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        if self.terminal_seen {
            return Err(RuntimeError::InvalidEvent {
                message: format!(
                    "refusing event {:?} after terminal event already emitted",
                    event.kind
                ),
            });
        }
        let is_terminal = event.is_terminal();
        self.inner.emit(event)?;
        if is_terminal {
            self.terminal_seen = true;
        }
        Ok(())
    }
}

/// Host ingress guard: request run id, strict monotonic sequence, payload
/// validation, single terminal, optional host cancel, and exactly-one-terminal
/// on finish.
pub struct ProtocolGuardSink<'a> {
    inner: &'a mut dyn EventSink,
    expected_run_id: RunId,
    next_sequence: u64,
    terminal_seen: bool,
    /// When set, host cancel wins: adapter events are replaced with
    /// `run_cancelled` and further emits return [`RuntimeError::Cancelled`].
    cancel: Option<&'a AtomicBool>,
}

impl<'a> ProtocolGuardSink<'a> {
    pub fn new(expected_run_id: RunId, inner: &'a mut dyn EventSink) -> Self {
        Self {
            inner,
            expected_run_id,
            next_sequence: 1,
            terminal_seen: false,
            cancel: None,
        }
    }

    /// Like [`new`](Self::new) but host cancel is checked on every emit.
    pub fn with_cancel(
        expected_run_id: RunId,
        inner: &'a mut dyn EventSink,
        cancel: &'a AtomicBool,
    ) -> Self {
        Self {
            inner,
            expected_run_id,
            next_sequence: 1,
            terminal_seen: false,
            cancel: Some(cancel),
        }
    }

    pub fn terminal_seen(&self) -> bool {
        self.terminal_seen
    }

    /// Call after the adapter returns successfully to enforce a terminal.
    pub fn finish(&self) -> Result<(), RuntimeError> {
        if self.terminal_seen {
            Ok(())
        } else {
            Err(RuntimeError::InvalidEvent {
                message: "turn ended without a terminal event".into(),
            })
        }
    }

    /// Emit a host-forced `run_cancelled` if no terminal has been delivered yet.
    pub fn force_run_cancelled(&mut self) -> Result<(), RuntimeError> {
        if self.terminal_seen {
            return Ok(());
        }
        let event = RuntimeEvent::new(
            self.expected_run_id,
            self.next_sequence,
            RuntimeEventKind::RunCancelled,
            RunCancelledPayload {
                reason: Some("host".into()),
            },
            Utc::now(),
        )?;
        self.emit_validated(event)
    }

    fn host_cancelled(&self) -> bool {
        self.cancel
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    fn emit_validated(&mut self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        if self.terminal_seen {
            return Err(RuntimeError::InvalidEvent {
                message: format!(
                    "refusing event {:?} after terminal event already emitted",
                    event.kind
                ),
            });
        }
        if event.run_id != self.expected_run_id {
            return Err(RuntimeError::InvalidEvent {
                message: format!(
                    "event run_id {} does not match request run_id {}",
                    event.run_id, self.expected_run_id
                ),
            });
        }
        if event.sequence != self.next_sequence {
            return Err(RuntimeError::InvalidEvent {
                message: format!(
                    "expected sequence {}, got {} (duplicate, skipped, or out-of-order)",
                    self.next_sequence, event.sequence
                ),
            });
        }
        let payload = validate_payload(event.kind, &event.payload)?;
        let event = RuntimeEvent {
            run_id: event.run_id,
            sequence: event.sequence,
            kind: event.kind,
            payload,
            created_at: event.created_at,
        };
        let is_terminal = event.is_terminal();
        self.inner.emit(event)?;
        self.next_sequence += 1;
        if is_terminal {
            self.terminal_seen = true;
        }
        Ok(())
    }
}

impl EventSink for ProtocolGuardSink<'_> {
    fn emit(&mut self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        if self.host_cancelled() {
            if !self.terminal_seen {
                self.force_run_cancelled()?;
            }
            return Err(RuntimeError::Cancelled);
        }
        self.emit_validated(event)
    }
}

/// Sink that delivers into a [`BoundedDelivery`] with blocking backpressure.
pub struct BoundedDeliverySink {
    delivery: std::sync::Arc<BoundedDelivery>,
}

impl BoundedDeliverySink {
    pub fn new(delivery: std::sync::Arc<BoundedDelivery>) -> Self {
        Self { delivery }
    }
}

impl EventSink for BoundedDeliverySink {
    fn emit(&mut self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        self.delivery.push_blocking(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keeper_agent_protocol::RuntimeEventKind;

    fn event(
        run: RunId,
        seq: u64,
        kind: RuntimeEventKind,
        payload: serde_json::Value,
    ) -> RuntimeEvent {
        RuntimeEvent {
            run_id: run,
            sequence: seq,
            kind,
            payload,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn terminal_guard_rejects_second_terminal() {
        let mut collect = CollectingSink::default();
        let mut guard = TerminalGuardSink::new(&mut collect);
        let run = RunId::new();
        let e1 = RuntimeEvent::new(
            run,
            1,
            RuntimeEventKind::TurnCompleted,
            serde_json::json!({"summary": "ok"}),
            Utc::now(),
        )
        .unwrap();
        guard.emit(e1).unwrap();
        let e2 = RuntimeEvent::new(
            run,
            2,
            RuntimeEventKind::RunCancelled,
            serde_json::json!({}),
            Utc::now(),
        )
        .unwrap();
        let err = guard.emit(e2).unwrap_err();
        assert!(matches!(err, RuntimeError::InvalidEvent { .. }));
        assert_eq!(collect.events.len(), 1);
    }

    #[test]
    fn protocol_guard_rejects_wrong_run_id() {
        let mut collect = CollectingSink::default();
        let expected = RunId::new();
        let mut guard = ProtocolGuardSink::new(expected, &mut collect);
        let err = guard
            .emit(event(
                RunId::new(),
                1,
                RuntimeEventKind::RunStarted,
                serde_json::json!({}),
            ))
            .unwrap_err();
        assert!(matches!(err, RuntimeError::InvalidEvent { .. }));
        assert!(err.to_string().contains("run_id"));
    }

    #[test]
    fn protocol_guard_rejects_skipped_and_duplicate_sequence() {
        let mut collect = CollectingSink::default();
        let run = RunId::new();
        let mut guard = ProtocolGuardSink::new(run, &mut collect);
        guard
            .emit(event(
                run,
                1,
                RuntimeEventKind::RunStarted,
                serde_json::json!({}),
            ))
            .unwrap();
        let skip = guard
            .emit(event(
                run,
                3,
                RuntimeEventKind::Heartbeat,
                serde_json::json!({"tick": 1}),
            ))
            .unwrap_err();
        assert!(skip.to_string().contains("sequence"));
        let dup = guard
            .emit(event(
                run,
                1,
                RuntimeEventKind::Heartbeat,
                serde_json::json!({"tick": 1}),
            ))
            .unwrap_err();
        assert!(dup.to_string().contains("sequence"));
    }

    #[test]
    fn protocol_guard_requires_terminal_on_finish() {
        let mut collect = CollectingSink::default();
        let run = RunId::new();
        let mut guard = ProtocolGuardSink::new(run, &mut collect);
        guard
            .emit(event(
                run,
                1,
                RuntimeEventKind::RunStarted,
                serde_json::json!({}),
            ))
            .unwrap();
        let err = guard.finish().unwrap_err();
        assert!(err.to_string().contains("terminal"));
    }

    #[test]
    fn protocol_guard_rejects_forbidden_payload_fields() {
        let mut collect = CollectingSink::default();
        let run = RunId::new();
        let mut guard = ProtocolGuardSink::new(run, &mut collect);
        let err = guard
            .emit(event(
                run,
                1,
                RuntimeEventKind::AssistantDelta,
                serde_json::json!({"text": "x", "api_key": "sk"}),
            ))
            .unwrap_err();
        assert!(matches!(err, RuntimeError::InvalidEvent { .. }));
        assert!(collect.events.is_empty());
    }

    #[test]
    fn protocol_guard_cancel_replaces_completion_with_run_cancelled() {
        let mut collect = CollectingSink::default();
        let run = RunId::new();
        let cancel = AtomicBool::new(false);
        {
            let mut guard = ProtocolGuardSink::with_cancel(run, &mut collect, &cancel);
            guard
                .emit(event(
                    run,
                    1,
                    RuntimeEventKind::RunStarted,
                    serde_json::json!({}),
                ))
                .unwrap();
            cancel.store(true, Ordering::SeqCst);
            let err = guard
                .emit(event(
                    run,
                    2,
                    RuntimeEventKind::TurnCompleted,
                    serde_json::json!({}),
                ))
                .unwrap_err();
            assert!(matches!(err, RuntimeError::Cancelled));
            guard.finish().unwrap();
        }
        assert_eq!(collect.events.len(), 2);
        assert_eq!(
            collect.events[1].kind,
            RuntimeEventKind::RunCancelled,
            "host cancel must win over turn_completed"
        );
    }
}
