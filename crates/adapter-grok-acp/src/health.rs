//! Adapter health / authentication probe state.
//!
//! Truth table (H1):
//! - executable absence → `unavailable` (never healthy from file presence alone)
//! - successful `initialize` → `transport_ready_auth_unverified`
//! - successful real prompt → `healthy`
//! - recognized auth failures → `authentication_failed` (clears healthy)

use keeper_agent_protocol::RuntimeError;

/// Stable probe detail codes. Secret-free; product UI may key off these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ProbeState {
    /// No transport proof yet (missing binary, never initialized, dead process).
    #[default]
    Unavailable,
    /// `initialize` succeeded; authentication has not been proven by a prompt.
    TransportReadyAuthUnverified,
    /// A real prompt completed successfully (auth usable).
    Healthy,
    /// A recognized authentication failure cleared healthy status.
    AuthenticationFailed,
}

impl ProbeState {
    pub(crate) fn detail_code(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::TransportReadyAuthUnverified => "transport_ready_auth_unverified",
            Self::Healthy => "healthy",
            Self::AuthenticationFailed => "authentication_failed",
        }
    }

    pub(crate) fn is_healthy(self) -> bool {
        matches!(self, Self::Healthy)
    }
}

/// Classify transport/RPC error text as a recognized authentication failure.
///
/// Matches stable, non-secret phrases only — never inspects credentials or
/// auth file contents.
pub(crate) fn is_auth_failure_message(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    const PATTERNS: &[&str] = &[
        "unauthenticated",
        "unauthorized",
        "authentication required",
        "authentication failed",
        "authentication_failed",
        "not authenticated",
        "auth failed",
        "auth_failed",
        "login required",
        "not logged in",
        "invalid token",
        "expired token",
        "token expired",
        "no credentials",
        "sign in required",
        "signin required",
    ];
    PATTERNS.iter().any(|p| m.contains(p))
}

pub(crate) fn is_auth_failure_error(err: &RuntimeError) -> bool {
    match err {
        RuntimeError::Internal { message } => is_auth_failure_message(message),
        other => is_auth_failure_message(&other.to_string()),
    }
}

/// Extract agent version from initialize result `_meta.agentVersion`.
pub(crate) fn extract_agent_version(result: &serde_json::Value) -> Option<String> {
    result
        .pointer("/_meta/agentVersion")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detail_codes_are_stable() {
        assert_eq!(ProbeState::Unavailable.detail_code(), "unavailable");
        assert_eq!(
            ProbeState::TransportReadyAuthUnverified.detail_code(),
            "transport_ready_auth_unverified"
        );
        assert_eq!(ProbeState::Healthy.detail_code(), "healthy");
        assert_eq!(
            ProbeState::AuthenticationFailed.detail_code(),
            "authentication_failed"
        );
        assert!(!ProbeState::TransportReadyAuthUnverified.is_healthy());
        assert!(ProbeState::Healthy.is_healthy());
    }

    #[test]
    fn auth_patterns_match_without_secret_material() {
        assert!(is_auth_failure_message(
            "acp_request_failed: unauthenticated"
        ));
        assert!(is_auth_failure_message("Unauthorized"));
        assert!(is_auth_failure_message("login required"));
        assert!(!is_auth_failure_message("permission denied for write"));
        assert!(!is_auth_failure_message("timeout"));
    }

    #[test]
    fn agent_version_from_initialize_meta() {
        let v = json!({
            "protocolVersion": 1,
            "_meta": { "agentVersion": "0.2.112" }
        });
        assert_eq!(extract_agent_version(&v).as_deref(), Some("0.2.112"));
        assert!(extract_agent_version(&json!({})).is_none());
    }
}
