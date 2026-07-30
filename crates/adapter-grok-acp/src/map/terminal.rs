//! stopReason / transport errors → terminal runtime events.

use chrono::{DateTime, Utc};
use keeper_agent_protocol::{
    RunCancelledPayload, RunId, RuntimeError, RuntimeEvent, RuntimeEventKind, TurnCompletedPayload,
    TurnFailedPayload,
};

/// Map ACP `stopReason` to a terminal event.
///
/// `cancelled` (including PermissionRejected) is always `RunCancelled`.
pub fn map_stop_reason(
    run_id: RunId,
    sequence: u64,
    stop_reason: &str,
    cancelled: bool,
    now: DateTime<Utc>,
) -> Result<RuntimeEvent, RuntimeError> {
    if cancelled || stop_reason.eq_ignore_ascii_case("cancelled") {
        return RuntimeEvent::new(
            run_id,
            sequence,
            RuntimeEventKind::RunCancelled,
            RunCancelledPayload {
                reason: Some(if stop_reason.eq_ignore_ascii_case("cancelled") {
                    "cancelled".into()
                } else {
                    "user".into()
                }),
            },
            now,
        );
    }

    if stop_reason.eq_ignore_ascii_case("end_turn")
        || stop_reason.eq_ignore_ascii_case("max_tokens")
    {
        return RuntimeEvent::new(
            run_id,
            sequence,
            RuntimeEventKind::TurnCompleted,
            TurnCompletedPayload { summary: None },
            now,
        );
    }

    // Unknown stop reasons are non-success diagnostics, not panics.
    RuntimeEvent::new(
        run_id,
        sequence,
        RuntimeEventKind::TurnFailed,
        TurnFailedPayload {
            code: "acp_stop_reason".into(),
            message: format!("unexpected stop reason: {stop_reason}"),
        },
        now,
    )
}

pub fn terminal_for_error(
    run_id: RunId,
    sequence: u64,
    err: &RuntimeError,
    cancelled: bool,
    now: DateTime<Utc>,
) -> Result<RuntimeEvent, RuntimeError> {
    if cancelled || matches!(err, RuntimeError::Cancelled) {
        return RuntimeEvent::new(
            run_id,
            sequence,
            RuntimeEventKind::RunCancelled,
            RunCancelledPayload {
                reason: Some("user".into()),
            },
            now,
        );
    }
    let (code, message) = match err {
        RuntimeError::Internal { message } => {
            let code = if message.starts_with("acp_") {
                message.split(':').next().unwrap_or("acp_error").to_string()
            } else {
                "adapter_error".into()
            };
            (code, sanitize_message(message))
        }
        other => ("adapter_error".into(), sanitize_message(&other.to_string())),
    };
    RuntimeEvent::new(
        run_id,
        sequence,
        RuntimeEventKind::TurnFailed,
        TurnFailedPayload { code, message },
        now,
    )
}

fn sanitize_message(message: &str) -> String {
    let mut out = message.chars().take(300).collect::<String>();
    // Never surface private ACP ids that might appear in raw errors.
    if out.to_ascii_lowercase().contains("sessionid") {
        out = "acp session error".into();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn rid() -> RunId {
        RunId(Uuid::nil())
    }

    #[test]
    fn end_turn_is_completed() {
        let e = map_stop_reason(rid(), 1, "end_turn", false, Utc::now()).unwrap();
        assert_eq!(e.kind, RuntimeEventKind::TurnCompleted);
    }

    #[test]
    fn cancelled_including_permission_rejected_is_cancelled() {
        let e = map_stop_reason(rid(), 1, "cancelled", false, Utc::now()).unwrap();
        assert_eq!(e.kind, RuntimeEventKind::RunCancelled);
    }

    #[test]
    fn unknown_stop_reason_is_failed() {
        let e = map_stop_reason(rid(), 1, "weird", false, Utc::now()).unwrap();
        assert_eq!(e.kind, RuntimeEventKind::TurnFailed);
    }
}
