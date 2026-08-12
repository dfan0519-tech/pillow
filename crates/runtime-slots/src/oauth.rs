//! Pure OIDC / OAuth helpers for slot auth (no network, no browser).
//!
//! Shape follows the verified OpenCode credential + PKCE flow (MIT): code
//! verifier/challenge (S256), authorize URL construction, token response
//! parsing, and access-token refresh decision with skew. Network exchange and
//! loopback callback are intentionally out of scope for this module.

use std::fmt;
use std::time::{Duration, SystemTime};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

use crate::RuntimeSlotError;
use crate::secret::SecretBytes;

/// Default proactive refresh skew (matches OpenCode xAI plugin: 120s).
pub const ACCESS_TOKEN_REFRESH_SKEW: Duration = Duration::from_secs(120);

/// PKCE pair (RFC 7636). Verifier is secret-bearing for the lifetime of the
/// authorization code exchange; challenge is public.
pub struct PkceCodes {
    pub verifier: SecretBytes,
    pub challenge: String,
}

impl fmt::Debug for PkceCodes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PkceCodes")
            .field("verifier", &"SecretBytes([REDACTED])")
            .field("challenge", &self.challenge)
            .finish()
    }
}

/// Parsed token endpoint response. Tokens never appear in Debug.
pub struct TokenResponse {
    pub access_token: SecretBytes,
    pub refresh_token: Option<SecretBytes>,
    pub expires_in: Option<u64>,
    pub token_type: Option<String>,
    pub scope: Option<String>,
}

impl fmt::Debug for TokenResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenResponse")
            .field("access_token", &"SecretBytes([REDACTED])")
            .field(
                "refresh_token",
                &self
                    .refresh_token
                    .as_ref()
                    .map(|_| "SecretBytes([REDACTED])"),
            )
            .field("expires_in", &self.expires_in)
            .field("token_type", &self.token_type)
            .field("scope", &self.scope)
            .finish()
    }
}

/// Generate a PKCE verifier (43 unpadded base64url chars) and S256 challenge.
pub fn generate_pkce() -> Result<PkceCodes, RuntimeSlotError> {
    let mut raw = [0u8; 32];
    getrandom::getrandom(&mut raw).map_err(|_| RuntimeSlotError::InvalidInput)?;
    let verifier = URL_SAFE_NO_PAD.encode(raw);
    let challenge = s256_challenge(&verifier);
    Ok(PkceCodes {
        verifier: SecretBytes::new(verifier),
        challenge,
    })
}

/// S256 code_challenge = BASE64URL-ENCODE(SHA256(ASCII(code_verifier))) without padding.
pub fn s256_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

/// Verify that `challenge` is the S256 transform of `verifier`.
pub fn verify_s256_challenge(verifier: &str, challenge: &str) -> bool {
    if verifier.len() < 43 || verifier.len() > 128 || challenge.is_empty() {
        return false;
    }
    s256_challenge(verifier) == challenge
}

/// Build an authorization URL for the code + PKCE flow (response_type=code).
pub fn build_authorize_url(
    authorization_endpoint: &str,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[String],
    code_challenge: &str,
    state: &str,
) -> Result<String, RuntimeSlotError> {
    if authorization_endpoint.trim().is_empty()
        || client_id.trim().is_empty()
        || redirect_uri.trim().is_empty()
        || code_challenge.trim().is_empty()
        || state.trim().is_empty()
        || scopes.is_empty()
    {
        return Err(RuntimeSlotError::InvalidInput);
    }
    let scope = scopes.join(" ");
    // Manual query construction keeps this pure (no URL crate dependency).
    let mut url = String::with_capacity(authorization_endpoint.len() + 256);
    url.push_str(authorization_endpoint);
    if authorization_endpoint.contains('?') {
        if !authorization_endpoint.ends_with('&') && !authorization_endpoint.ends_with('?') {
            url.push('&');
        }
    } else {
        url.push('?');
    }
    push_query(&mut url, "response_type", "code");
    url.push('&');
    push_query(&mut url, "client_id", client_id);
    url.push('&');
    push_query(&mut url, "redirect_uri", redirect_uri);
    url.push('&');
    push_query(&mut url, "scope", &scope);
    url.push('&');
    push_query(&mut url, "code_challenge", code_challenge);
    url.push('&');
    push_query(&mut url, "code_challenge_method", "S256");
    url.push('&');
    push_query(&mut url, "state", state);
    Ok(url)
}

/// application/x-www-form-urlencoded body for authorization_code exchange.
pub fn build_code_exchange_form(
    client_id: &str,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<String, RuntimeSlotError> {
    if client_id.is_empty()
        || code.is_empty()
        || redirect_uri.is_empty()
        || code_verifier.is_empty()
    {
        return Err(RuntimeSlotError::InvalidInput);
    }
    let mut body = String::new();
    push_query(&mut body, "grant_type", "authorization_code");
    body.push('&');
    push_query(&mut body, "client_id", client_id);
    body.push('&');
    push_query(&mut body, "code", code);
    body.push('&');
    push_query(&mut body, "redirect_uri", redirect_uri);
    body.push('&');
    push_query(&mut body, "code_verifier", code_verifier);
    Ok(body)
}

/// application/x-www-form-urlencoded body for refresh_token grant.
///
/// Caller must zero any buffers that held the refresh token after use.
pub fn build_refresh_token_form(
    client_id: &str,
    refresh_token: &str,
) -> Result<String, RuntimeSlotError> {
    if client_id.is_empty() || refresh_token.is_empty() {
        return Err(RuntimeSlotError::InvalidInput);
    }
    let mut body = String::new();
    push_query(&mut body, "grant_type", "refresh_token");
    body.push('&');
    push_query(&mut body, "client_id", client_id);
    body.push('&');
    push_query(&mut body, "refresh_token", refresh_token);
    Ok(body)
}

/// Parse a token endpoint JSON body. Rejects missing access_token.
///
/// Bad JSON → [`RuntimeSlotError::OauthTokenBadJson`].
/// Missing/empty `access_token` → [`RuntimeSlotError::OauthTokenMissingField`].
pub fn parse_token_response(body: &str) -> Result<TokenResponse, RuntimeSlotError> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|_| RuntimeSlotError::OauthTokenBadJson)?;
    let access = value
        .get("access_token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or(RuntimeSlotError::OauthTokenMissingField)?;
    let refresh = value
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| SecretBytes::new(s.to_owned()));
    let expires_in = value.get("expires_in").and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_i64().and_then(|n| u64::try_from(n).ok()))
    });
    let token_type = value
        .get("token_type")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let scope = value
        .get("scope")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    Ok(TokenResponse {
        access_token: SecretBytes::new(access.to_owned()),
        refresh_token: refresh,
        expires_in,
        token_type,
        scope,
    })
}

/// Absolute expiry from `expires_in` seconds relative to `now`.
pub fn expiry_from_expires_in(now: SystemTime, expires_in: u64) -> SystemTime {
    now + Duration::from_secs(expires_in)
}

/// Whether the access token should be refreshed before use.
///
/// Returns `true` when `expires_at <= now + skew` (inclusive boundary).
/// If `now + skew` overflows, conservatively refresh.
pub fn access_token_needs_refresh(expires_at: SystemTime, now: SystemTime, skew: Duration) -> bool {
    match now.checked_add(skew) {
        Some(threshold) => expires_at <= threshold,
        None => true,
    }
}

/// Convenience: needs refresh using [`ACCESS_TOKEN_REFRESH_SKEW`] and wall clock.
pub fn access_token_needs_refresh_now(expires_at: SystemTime) -> bool {
    access_token_needs_refresh(expires_at, SystemTime::now(), ACCESS_TOKEN_REFRESH_SKEW)
}

fn push_query(out: &mut String, key: &str, value: &str) {
    out.push_str(&form_encode(key));
    out.push('=');
    out.push_str(&form_encode(value));
}

fn form_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => {
                encoded.push('%');
                encoded.push(hex_digit(byte >> 4));
                encoded.push(hex_digit(byte & 0x0f));
            }
        }
    }
    encoded
}

fn hex_digit(nibble: u8) -> char {
    char::from(if nibble < 10 {
        b'0' + nibble
    } else {
        b'A' + (nibble - 10)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn runtime_token(prefix: &str, suffix: &str) -> String {
        // Concatenate at runtime so source never holds a secret-shaped literal.
        format!("{prefix}{suffix}")
    }

    #[test]
    fn pkce_generate_and_verify_round_trip() {
        let pkce = generate_pkce().expect("pkce");
        let verifier = std::str::from_utf8(pkce.verifier.as_bytes()).expect("utf8");
        assert!(verifier.len() >= 43);
        assert!(verify_s256_challenge(verifier, &pkce.challenge));
        assert!(!verify_s256_challenge(verifier, "not-the-challenge"));
        assert!(!verify_s256_challenge("short", &pkce.challenge));
    }

    #[test]
    fn s256_challenge_is_stable_for_fixed_verifier() {
        let verifier = runtime_token(
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
            "0123456789-._~",
        );
        // Truncate to 43 chars (RFC minimum example length).
        let verifier: String = verifier.chars().take(43).collect();
        let first = s256_challenge(&verifier);
        let second = s256_challenge(&verifier);
        assert_eq!(first, second);
        assert!(!first.contains('='));
        assert!(verify_s256_challenge(&verifier, &first));
    }

    #[test]
    fn authorize_url_contains_pkce_and_scopes() {
        let url = build_authorize_url(
            "https://issuer.example.invalid/authorize",
            "client-1",
            "http://127.0.0.1:9/callback",
            &["openid".into(), "offline_access".into()],
            "challenge-value",
            "state-value",
        )
        .expect("url");
        assert!(url.starts_with("https://issuer.example.invalid/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("code_challenge=challenge-value"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(
            url.contains("scope=openid+offline_access")
                || url.contains("scope=openid%20offline_access")
                || url.contains("scope=openid+offline_access")
        );
        assert!(url.contains("state=state-value"));
        assert!(url.contains("client_id=client-1"));
    }

    #[test]
    fn parse_token_response_positive_and_negative() {
        let access = runtime_token("acc-", "ess-material-for-test");
        let refresh = runtime_token("ref-", "resh-material-for-test");
        let body = format!(
            r#"{{"access_token":"{access}","refresh_token":"{refresh}","expires_in":3600,"token_type":"Bearer"}}"#
        );
        let parsed = parse_token_response(&body).expect("parse");
        assert_eq!(parsed.access_token.as_bytes(), access.as_bytes());
        assert_eq!(
            parsed.refresh_token.as_ref().map(|s| s.as_bytes()),
            Some(refresh.as_bytes())
        );
        assert_eq!(parsed.expires_in, Some(3600));
        assert_eq!(parsed.token_type.as_deref(), Some("Bearer"));

        assert!(matches!(
            parse_token_response(r#"{"expires_in":1}"#),
            Err(RuntimeSlotError::OauthTokenMissingField)
        ));
        assert!(matches!(
            parse_token_response("not-json"),
            Err(RuntimeSlotError::OauthTokenBadJson)
        ));
        assert!(matches!(
            parse_token_response(r#"{"access_token":""}"#),
            Err(RuntimeSlotError::OauthTokenMissingField)
        ));
    }

    #[test]
    fn refresh_decision_positive_negative_and_boundary() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        let skew = Duration::from_secs(120);

        // Far future → no refresh.
        let far = now + Duration::from_secs(3_600);
        assert!(!access_token_needs_refresh(far, now, skew));

        // Already expired → refresh.
        let past = now - Duration::from_secs(1);
        assert!(access_token_needs_refresh(past, now, skew));

        // Exactly at now → refresh.
        assert!(access_token_needs_refresh(now, now, skew));

        // Exactly at now + skew (inclusive boundary) → refresh.
        let at_skew = now + skew;
        assert!(access_token_needs_refresh(at_skew, now, skew));

        // One second past skew threshold → still valid.
        let just_after = now + skew + Duration::from_secs(1);
        assert!(!access_token_needs_refresh(just_after, now, skew));
    }

    #[test]
    fn token_response_debug_redacts_secrets() {
        let access = runtime_token("acc-", "ess-debug-marker");
        let refresh = runtime_token("ref-", "resh-debug-marker");
        let body =
            format!(r#"{{"access_token":"{access}","refresh_token":"{refresh}","expires_in":60}}"#);
        let parsed = parse_token_response(&body).expect("parse");
        let debug = format!("{parsed:?}");
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains(&access));
        assert!(!debug.contains(&refresh));
    }

    #[test]
    fn refresh_form_includes_grant_type_not_logged_as_dto() {
        let refresh = runtime_token("ref-", "resh-form-marker");
        let form = build_refresh_token_form("client-1", &refresh).expect("form");
        assert!(form.contains("grant_type=refresh_token"));
        assert!(form.contains("client_id=client-1"));
        assert!(form.contains(&form_encode(&refresh)) || form.contains(&refresh));
    }
}
