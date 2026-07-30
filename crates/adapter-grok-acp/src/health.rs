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
        .filter(|s| is_agent_version(s))
        .map(str::to_owned)
}

/// Provider metadata may describe a release, but must never become an identity
/// or machine-detail channel in the health surface.
fn is_agent_version(value: &str) -> bool {
    const MAX_AGENT_VERSION_BYTES: usize = 64;

    if value.is_empty() || value.len() > MAX_AGENT_VERSION_BYTES || !value.is_ascii() {
        return false;
    }

    let bytes = value.as_bytes();
    let starts_like_version = bytes[0].is_ascii_digit()
        || (bytes[0] == b'v' && bytes.get(1).is_some_and(u8::is_ascii_digit));
    starts_like_version
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
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

    #[test]
    fn agent_version_rejects_identity_and_non_version_metadata() {
        let too_long = "1".repeat(65);
        for invalid in [
            "person@example.com",
            r"C:\\Users\\person",
            "../agent",
            "1.2.3\nsecret",
            "v",
            "x1.2.3",
            too_long.as_str(),
        ] {
            assert!(
                extract_agent_version(&json!({ "_meta": { "agentVersion": invalid } })).is_none(),
                "must reject {invalid:?}"
            );
        }
    }

    #[test]
    fn agent_version_accepts_short_ascii_version_values() {
        for version in ["1", "1.2.3", "v1.2.3", "0.0.0-fake", "1.0.0+build.7"] {
            assert_eq!(
                extract_agent_version(&json!({ "_meta": { "agentVersion": version } })).as_deref(),
                Some(version)
            );
        }
    }
}
