//! Event sinks used by the host and tests.

use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Utc;
use keeper_agent_protocol::{
    EventSink, RunCancelledPayload, RunId, RuntimeError, RuntimeEvent, RuntimeEventKind,
    validate_payload,
};

use crate::delivery::{BoundedDelivery, is_safe_post_cancel_write_resolution};

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
    ///
    /// Exception: at most one narrowly validated
    /// `write_proposal_resolved` (`cancelled`/`cancelled` or
    /// `interrupted`/`interrupted`) may commit before the host-forced
    /// terminal so the UI can event-drive pending-proposal refresh.
    cancel: Option<&'a AtomicBool>,
    /// Whether the single post-cancel safe write resolution already committed.
    post_cancel_safe_resolution_emitted: bool,
}

impl<'a> ProtocolGuardSink<'a> {
    pub fn new(expected_run_id: RunId, inner: &'a mut dyn EventSink) -> Self {
        Self {
            inner,
            expected_run_id,
            next_sequence: 1,
            terminal_seen: false,
            cancel: None,
            post_cancel_safe_resolution_emitted: false,
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
            post_cancel_safe_resolution_emitted: false,
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
            // Permit at most one safe write_proposal_resolved before the
            // host-forced run_cancelled. Everything else fails closed.
            if !self.terminal_seen
                && !self.post_cancel_safe_resolution_emitted
                && is_safe_post_cancel_write_resolution(&event)
                && self.emit_validated(event).is_ok()
            {
                self.post_cancel_safe_resolution_emitted = true;
            }
            if !self.terminal_seen {
                self.force_run_cancelled()?;
            }
            return Err(RuntimeError::Cancelled);
        }
        match self.emit_validated(event) {
            Ok(()) => Ok(()),
            // Delivery rejected a blocked non-cancel terminal after host cancel
            // won commit arbitration — force run_cancelled if still needed.
            Err(RuntimeError::Cancelled) => {
                if !self.terminal_seen {
                    self.force_run_cancelled()?;
                }
                Err(RuntimeError::Cancelled)
            }
            Err(e) => Err(e),
        }
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

    fn write_resolved_payload(state: &str, error_code: Option<&str>) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert("proposal_id".into(), serde_json::json!("wp-1"));
        map.insert("path".into(), serde_json::json!("note.txt"));
        map.insert("mode".into(), serde_json::json!("create"));
        map.insert("state".into(), serde_json::json!(state));
        if let Some(code) = error_code {
            map.insert("error_code".into(), serde_json::json!(code));
        }
        serde_json::Value::Object(map)
    }

    /// C3-F: after host cancel, exactly one safe write_proposal_resolved
    /// (cancelled/cancelled) must commit before the host-forced run_cancelled.
    #[test]
    fn post_cancel_safe_write_resolution_cancelled_before_run_cancelled() {
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
                    RuntimeEventKind::WriteProposalResolved,
                    write_resolved_payload("cancelled", Some("cancelled")),
                ))
                .unwrap_err();
            assert!(matches!(err, RuntimeError::Cancelled));
            guard.finish().unwrap();
        }
        let kinds: Vec<_> = collect.events.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                RuntimeEventKind::RunStarted,
                RuntimeEventKind::WriteProposalResolved,
                RuntimeEventKind::RunCancelled,
            ],
            "safe cancelled resolution must precede host run_cancelled; got {kinds:?}"
        );
        assert_eq!(
            collect.events[1]
                .payload
                .get("state")
                .and_then(|v| v.as_str()),
            Some("cancelled")
        );
        assert_eq!(
            collect.events[1]
                .payload
                .get("error_code")
                .and_then(|v| v.as_str()),
            Some("cancelled")
        );
    }

    /// C3-F: interrupted/interrupted is the other permitted post-cancel pair.
    #[test]
    fn post_cancel_safe_write_resolution_interrupted_before_run_cancelled() {
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
                    RuntimeEventKind::WriteProposalResolved,
                    write_resolved_payload("interrupted", Some("interrupted")),
                ))
                .unwrap_err();
            assert!(matches!(err, RuntimeError::Cancelled));
            guard.finish().unwrap();
        }
        let kinds: Vec<_> = collect.events.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                RuntimeEventKind::RunStarted,
                RuntimeEventKind::WriteProposalResolved,
                RuntimeEventKind::RunCancelled,
            ]
        );
    }

    /// C3-F negatives: success / mismatched / malformed / duplicate / ordinary
    /// post-cancel events must not commit; only host run_cancelled lands.
    #[test]
    fn post_cancel_blocks_success_mismatch_malformed_duplicate_and_ordinary() {
        // Success write resolution after cancel.
        {
            let mut collect = CollectingSink::default();
            let run = RunId::new();
            let cancel = AtomicBool::new(true);
            let mut guard = ProtocolGuardSink::with_cancel(run, &mut collect, &cancel);
            let err = guard
                .emit(event(
                    run,
                    1,
                    RuntimeEventKind::WriteProposalResolved,
                    write_resolved_payload("succeeded", None),
                ))
                .unwrap_err();
            assert!(matches!(err, RuntimeError::Cancelled));
            assert_eq!(collect.events.len(), 1);
            assert_eq!(collect.events[0].kind, RuntimeEventKind::RunCancelled);
        }

        // Rejected write resolution after cancel.
        {
            let mut collect = CollectingSink::default();
            let run = RunId::new();
            let cancel = AtomicBool::new(true);
            let mut guard = ProtocolGuardSink::with_cancel(run, &mut collect, &cancel);
            let _ = guard.emit(event(
                run,
                1,
                RuntimeEventKind::WriteProposalResolved,
                write_resolved_payload("rejected", Some("approval_rejected")),
            ));
            assert_eq!(collect.events.len(), 1);
            assert_eq!(collect.events[0].kind, RuntimeEventKind::RunCancelled);
        }

        // Mismatched state/error pair (cancelled / interrupted).
        {
            let mut collect = CollectingSink::default();
            let run = RunId::new();
            let cancel = AtomicBool::new(true);
            let mut guard = ProtocolGuardSink::with_cancel(run, &mut collect, &cancel);
            let _ = guard.emit(event(
                run,
                1,
                RuntimeEventKind::WriteProposalResolved,
                write_resolved_payload("cancelled", Some("interrupted")),
            ));
            assert_eq!(collect.events.len(), 1);
            assert_eq!(collect.events[0].kind, RuntimeEventKind::RunCancelled);
        }

        // Malformed payload (missing required fields).
        {
            let mut collect = CollectingSink::default();
            let run = RunId::new();
            let cancel = AtomicBool::new(true);
            let mut guard = ProtocolGuardSink::with_cancel(run, &mut collect, &cancel);
            let _ = guard.emit(event(
                run,
                1,
                RuntimeEventKind::WriteProposalResolved,
                serde_json::json!({"state": "cancelled", "error_code": "cancelled"}),
            ));
            assert_eq!(collect.events.len(), 1);
            assert_eq!(collect.events[0].kind, RuntimeEventKind::RunCancelled);
        }

        // Ordinary tool output after cancel.
        {
            let mut collect = CollectingSink::default();
            let run = RunId::new();
            let cancel = AtomicBool::new(true);
            let mut guard = ProtocolGuardSink::with_cancel(run, &mut collect, &cancel);
            let _ = guard.emit(event(
                run,
                1,
                RuntimeEventKind::ToolCallOutput,
                serde_json::json!({"tool_call_id": "t1", "chunk": "x"}),
            ));
            assert_eq!(collect.events.len(), 1);
            assert_eq!(collect.events[0].kind, RuntimeEventKind::RunCancelled);
        }

        // Duplicate safe resolution: only first commits; second is blocked.
        {
            let mut collect = CollectingSink::default();
            let run = RunId::new();
            let cancel = AtomicBool::new(false);
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
            let err1 = guard
                .emit(event(
                    run,
                    2,
                    RuntimeEventKind::WriteProposalResolved,
                    write_resolved_payload("cancelled", Some("cancelled")),
                ))
                .unwrap_err();
            assert!(matches!(err1, RuntimeError::Cancelled));
            // Second attempt after terminal: must not add another resolution.
            let err2 = guard
                .emit(event(
                    run,
                    3,
                    RuntimeEventKind::WriteProposalResolved,
                    write_resolved_payload("cancelled", Some("cancelled")),
                ))
                .unwrap_err();
            assert!(matches!(err2, RuntimeError::Cancelled));
            let resolved_count = collect
                .events
                .iter()
                .filter(|e| e.kind == RuntimeEventKind::WriteProposalResolved)
                .count();
            assert_eq!(resolved_count, 1, "at most one post-cancel safe resolution");
            assert_eq!(
                collect.events.last().map(|e| e.kind),
                Some(RuntimeEventKind::RunCancelled)
            );
            assert_eq!(
                collect
                    .events
                    .iter()
                    .filter(|e| e.kind == RuntimeEventKind::RunCancelled)
                    .count(),
                1,
                "exactly one host-forced run_cancelled"
            );
        }
    }

    /// C3-F: delivery+guard stack — safe resolution enqueues before cancel terminal.
    #[test]
    fn post_cancel_safe_resolution_through_bounded_delivery_before_run_cancelled() {
        let delivery = BoundedDelivery::new(8);
        let mut sink = BoundedDeliverySink::new(std::sync::Arc::clone(&delivery));
        let run = RunId::new();
        let cancel = AtomicBool::new(false);
        {
            let mut guard = ProtocolGuardSink::with_cancel(run, &mut sink, &cancel);
            guard
                .emit(event(
                    run,
                    1,
                    RuntimeEventKind::RunStarted,
                    serde_json::json!({}),
                ))
                .unwrap();
            // Host cancel wins on both AtomicBool and delivery queue.
            delivery.request_cancel();
            cancel.store(true, Ordering::SeqCst);
            let err = guard
                .emit(event(
                    run,
                    2,
                    RuntimeEventKind::WriteProposalResolved,
                    write_resolved_payload("cancelled", Some("cancelled")),
                ))
                .unwrap_err();
            assert!(matches!(err, RuntimeError::Cancelled));
            guard.finish().unwrap();
        }
        let events = delivery.drain();
        let kinds: Vec<_> = events.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                RuntimeEventKind::RunStarted,
                RuntimeEventKind::WriteProposalResolved,
                RuntimeEventKind::RunCancelled,
            ],
            "bounded delivery must commit safe resolution before run_cancelled; got {kinds:?}"
        );
    }
}
