//! Minimal JWT payload parsing for derived headers (no signature verify).
//!
//! Used only to extract non-secret claim fields such as ChatGPT account id.
//! Invalid tokens yield `None` — callers treat that as derived-header failure.

use base64::Engine;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use serde_json::Value;

/// Decode the middle segment of a compact JWT (`header.payload.sig`) as JSON.
///
/// Does **not** verify signatures. Returns `None` on malformed input.
pub fn parse_jwt_payload_json(token: &str) -> Option<Value> {
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload_b64 = parts.next()?;
    let _sig = parts.next()?;
    // Reject extra segments.
    if parts.next().is_some() {
        return None;
    }
    if payload_b64.is_empty() {
        return None;
    }
    let raw = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .or_else(|_| URL_SAFE.decode(payload_b64))
        .ok()?;
    serde_json::from_slice(&raw).ok()
}

/// Extract ChatGPT account id from JWT claims.
///
/// Source (OpenCode MIT, `packages/opencode/src/plugin/openai/codex.ts`):
/// - `claims.chatgpt_account_id`
/// - `claims["https://api.openai.com/auth"].chatgpt_account_id`
/// - `claims.organizations[0].id`
pub fn extract_chatgpt_account_id(claims: &Value) -> Option<String> {
    if let Some(id) = claims.get("chatgpt_account_id").and_then(Value::as_str)
        && !id.is_empty()
    {
        return Some(id.to_owned());
    }
    if let Some(id) = claims
        .get("https://api.openai.com/auth")
        .and_then(|v| v.get("chatgpt_account_id"))
        .and_then(Value::as_str)
        && !id.is_empty()
    {
        return Some(id.to_owned());
    }
    if let Some(id) = claims
        .get("organizations")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|org| org.get("id"))
        .and_then(Value::as_str)
        && !id.is_empty()
    {
        return Some(id.to_owned());
    }
    None
}

/// Resolve account id from OAuth material: explicit field, then id_token, then access_token JWT.
pub fn resolve_chatgpt_account_id(
    explicit: Option<&str>,
    id_token: Option<&str>,
    access_token: &str,
) -> Option<String> {
    if let Some(id) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        return Some(id.to_owned());
    }
    if let Some(tok) = id_token
        && let Some(claims) = parse_jwt_payload_json(tok)
        && let Some(id) = extract_chatgpt_account_id(&claims)
    {
        return Some(id);
    }
    if let Some(claims) = parse_jwt_payload_json(access_token) {
        return extract_chatgpt_account_id(&claims);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use serde_json::json;

    fn fake_jwt(payload: &Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let body = URL_SAFE_NO_PAD.encode(payload.to_string().as_bytes());
        // Signature segment is ignored; use a non-secret placeholder.
        format!("{header}.{body}.sig")
    }

    #[test]
    fn extracts_top_level_account_id() {
        let jwt = fake_jwt(&json!({"chatgpt_account_id": "acct-top"}));
        let claims = parse_jwt_payload_json(&jwt).expect("parse");
        assert_eq!(
            extract_chatgpt_account_id(&claims).as_deref(),
            Some("acct-top")
        );
    }

    #[test]
    fn extracts_nested_auth_claim() {
        let jwt = fake_jwt(&json!({
            "https://api.openai.com/auth": { "chatgpt_account_id": "acct-nested" }
        }));
        let claims = parse_jwt_payload_json(&jwt).expect("parse");
        assert_eq!(
            extract_chatgpt_account_id(&claims).as_deref(),
            Some("acct-nested")
        );
    }

    #[test]
    fn extracts_org_id_fallback() {
        let jwt = fake_jwt(&json!({
            "organizations": [{ "id": "org-1" }, { "id": "org-2" }]
        }));
        let claims = parse_jwt_payload_json(&jwt).expect("parse");
        assert_eq!(
            extract_chatgpt_account_id(&claims).as_deref(),
            Some("org-1")
        );
    }

    #[test]
    fn malformed_jwt_returns_none() {
        assert!(parse_jwt_payload_json("not-a-jwt").is_none());
        assert!(parse_jwt_payload_json("a.b").is_none());
        assert!(parse_jwt_payload_json("").is_none());
    }

    #[test]
    fn explicit_account_id_wins() {
        let jwt = fake_jwt(&json!({"chatgpt_account_id": "from-jwt"}));
        let resolved = resolve_chatgpt_account_id(Some("from-explicit"), Some(&jwt), "opaque");
        assert_eq!(resolved.as_deref(), Some("from-explicit"));
    }
}
