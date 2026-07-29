//! Event sinks used by the host and tests.

use keeper_agent_protocol::{EventSink, RuntimeError, RuntimeEvent};

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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use keeper_agent_protocol::{RunId, RuntimeEventKind};

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
}
