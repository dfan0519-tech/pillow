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
    let message = sanitize_message(&format!("unexpected stop reason: {stop_reason}"));
    RuntimeEvent::new(
        run_id,
        sequence,
        RuntimeEventKind::TurnFailed,
        TurnFailedPayload {
            code: "acp_stop_reason".into(),
            message,
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
            let code = public_error_code(message);
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

const REDACTED_MESSAGE: &str = "provider error details withheld";
const MAX_PUBLIC_MESSAGE_CHARS: usize = 300;

fn sanitize_message(message: &str) -> String {
    if contains_sensitive_data(message) {
        return REDACTED_MESSAGE.into();
    }

    message.chars().take(MAX_PUBLIC_MESSAGE_CHARS).collect()
}

fn public_error_code(message: &str) -> String {
    let Some(code) = message.strip_prefix("acp_").and_then(|rest| {
        let end = rest.find(':').unwrap_or(rest.len());
        message.get(..end + "acp_".len())
    }) else {
        return "adapter_error".into();
    };

    if code.len() <= 64
        && code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        && !contains_sensitive_data(code)
    {
        code.into()
    } else {
        "adapter_error".into()
    }
}

fn contains_sensitive_data(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    [
        "sessionid",
        "authorization",
        "bearer",
        "api key",
        "api_key",
        "token",
        "password",
        "secret",
        "auth.json",
        ".grok",
        r"c:\users\",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        || contains_obvious_email(message)
}

fn contains_obvious_email(message: &str) -> bool {
    let bytes = message.as_bytes();
    bytes.iter().enumerate().any(|(at, byte)| {
        if *byte != b'@' {
            return false;
        }

        let mut start = at;
        while start > 0 && is_email_local_byte(bytes[start - 1]) {
            start -= 1;
        }

        let mut end = at + 1;
        while end < bytes.len() && is_email_domain_byte(bytes[end]) {
            end += 1;
        }
        while end > at + 1 && bytes[end - 1] == b'.' {
            end -= 1;
        }

        let local = &bytes[start..at];
        let domain = &bytes[at + 1..end];
        let Some(last_dot) = domain.iter().rposition(|domain_byte| *domain_byte == b'.') else {
            return false;
        };
        let suffix = &domain[last_dot + 1..];

        !local.is_empty()
            && last_dot > 0
            && suffix.len() >= 2
            && suffix.iter().all(u8::is_ascii_alphabetic)
    })
}

fn is_email_local_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'%' | b'+' | b'-')
}

fn is_email_domain_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn rid() -> RunId {
        RunId(Uuid::nil())
    }

    fn failed_message(event: &RuntimeEvent) -> &str {
        event.payload["message"]
            .as_str()
            .expect("TurnFailed message")
    }

    fn terminal_error(message: impl Into<String>) -> RuntimeEvent {
        terminal_for_error(
            rid(),
            1,
            &RuntimeError::Internal {
                message: message.into(),
            },
            false,
            Utc::now(),
        )
        .unwrap()
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

    #[test]
    fn bearer_credentials_are_replaced_before_runtime_event_creation() {
        let event = terminal_error("acp_transport: Authorization: Bearer sk-live-secret");

        assert_eq!(failed_message(&event), "provider error details withheld");
        assert!(
            !serde_json::to_string(&event)
                .unwrap()
                .contains("sk-live-secret")
        );
    }

    #[test]
    fn api_keys_are_replaced_before_runtime_event_creation() {
        let event = terminal_error("request failed with API key sk-provider-secret");

        assert_eq!(failed_message(&event), "provider error details withheld");
    }

    #[test]
    fn credential_file_and_identity_markers_are_replaced() {
        for message in [
            "authorization header rejected",
            "refresh token rejected",
            "password invalid",
            "client secret invalid",
            "failed to read auth.json",
            "failed to read .grok settings",
            "api_key rejected",
        ] {
            let event = terminal_error(message);

            assert_eq!(
                failed_message(&event),
                "provider error details withheld",
                "message was not redacted: {message}"
            );
        }
    }

    #[test]
    fn sensitive_error_codes_are_not_written_to_runtime_events() {
        let event = terminal_error("acp_token_sk-live-secret: rejected");
        let serialized = serde_json::to_string(&event).unwrap();

        assert_eq!(failed_message(&event), "provider error details withheld");
        assert!(!serialized.contains("sk-live-secret"));
    }

    #[test]
    fn email_identities_are_replaced_before_runtime_event_creation() {
        let event = terminal_error("account owner@example.com was rejected");

        assert_eq!(failed_message(&event), "provider error details withheld");
    }

    #[test]
    fn sentence_final_email_identities_are_replaced() {
        let event = terminal_error("account rejected: owner@example.com.");

        assert_eq!(failed_message(&event), "provider error details withheld");
    }

    #[test]
    fn local_windows_paths_in_stop_reasons_are_replaced() {
        let event = map_stop_reason(
            rid(),
            1,
            r#"C:\Users\Alice\.grok\auth.json"#,
            false,
            Utc::now(),
        )
        .unwrap();

        assert_eq!(failed_message(&event), "provider error details withheld");
        assert!(
            !serde_json::to_string(&event)
                .unwrap()
                .contains(r#"C:\Users\Alice"#)
        );
    }

    #[test]
    fn ordinary_provider_errors_remain_actionable() {
        let event = terminal_error("acp_transport: provider temporarily unavailable");

        assert_eq!(
            failed_message(&event),
            "acp_transport: provider temporarily unavailable"
        );
    }

    #[test]
    fn ordinary_provider_errors_are_limited_to_300_characters() {
        let event = terminal_error("x".repeat(400));

        assert_eq!(failed_message(&event).chars().count(), 300);
        assert_eq!(failed_message(&event), "x".repeat(300));
    }
}
