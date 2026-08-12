//! Adapter description model and prepared-request types.

use std::fmt;

use crate::credential::CredentialKind;
use crate::redact::{is_sensitive_header_name, redact_header_value};

/// Wire protocol shape the request body is expected to follow.
///
/// This crate does not validate body schema; the tag exists so the host can
/// select encoders / stream parsers when wiring a real transport later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolShape {
    /// OpenAI Chat Completions (`POST …/chat/completions`).
    OpenAiChatCompletions,
    /// OpenAI Responses API shape (including proprietary Codex backend).
    OpenAiResponses,
    /// Anthropic Messages API.
    AnthropicMessages,
    /// Other / provider-specific — host must know how to encode.
    Other,
}

/// How authentication material is injected into request headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthInjection {
    /// Set `Authorization: Bearer <token>` (standard OpenAI-compatible).
    StandardBearer,
    /// Remove any standard `Authorization` header, then set a custom bearer
    /// header (name usually still `Authorization`). Used when the transport
    /// layer may have pre-filled a dummy key that must not ship.
    CustomBearer {
        /// Header that receives `Bearer <access>` after the strip step.
        header_name: String,
        /// When true, drop existing Authorization headers first.
        strip_standard_authorization: bool,
    },
}

/// Header whose value is derived from credential claims, not a constant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DerivedHeader {
    /// ChatGPT account id (`ChatGPT-Account-Id` in the OpenCode Codex plugin).
    ///
    /// Resolved from explicit `Credential::OAuth.account_id`, else JWT claims
    /// on `id_token` / `access_token` (`chatgpt_account_id` paths).
    ChatGptAccountId { header_name: String },
}

/// Full description of one provider’s request surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAdapter {
    pub provider_id: String,
    pub display_name: String,
    /// Absolute request URL (may be non-standard, e.g. Codex backend).
    pub endpoint_url: String,
    pub auth: AuthInjection,
    /// Headers with fixed values (e.g. `Content-Type`).
    pub constant_headers: Vec<(String, String)>,
    /// Headers derived from credentials; missing source → prepare **rejects**.
    pub derived_headers: Vec<DerivedHeader>,
    /// Empty = no model restriction. Non-empty = only listed ids are accepted
    /// (no silent substitution).
    pub model_allowlist: Vec<String>,
    pub protocol: ProtocolShape,
    pub required_credential: CredentialKind,
}

/// HTTP headers of a prepared request. Debug output redacts secrets.
#[derive(Clone, PartialEq, Eq)]
pub struct RequestHeaders {
    entries: Vec<(String, String)>,
}

impl RequestHeaders {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub(crate) fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        // Replace existing same-name (case-insensitive) so strip+set is stable.
        self.entries.retain(|(k, _)| !k.eq_ignore_ascii_case(&name));
        self.entries.push((name, value));
    }

    pub(crate) fn remove_authorization(&mut self) {
        self.entries
            .retain(|(k, _)| !k.eq_ignore_ascii_case("authorization"));
    }

    /// Immutable view of header pairs (name, value) for the transport layer.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl fmt::Debug for RequestHeaders {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        for (name, value) in &self.entries {
            let shown = if is_sensitive_header_name(name) {
                redact_header_value(value)
            } else {
                // Still scrub bearer-shaped values if they land under odd names.
                if value_looks_like_bearer_or_token(value) {
                    redact_header_value(value)
                } else {
                    value.clone()
                }
            };
            list.entry(&format!("{name}: {shown}"));
        }
        list.finish()
    }
}

fn value_looks_like_bearer_or_token(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.len() < 8 {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("bearer ")
        || lower.starts_with("sk-")
        || (trimmed.matches('.').count() == 2 && trimmed.starts_with("eyJ"))
}

/// Output of a successful [`crate::prepare_request`] call.
///
/// Ready for a host HTTP client. This crate never sends it.
#[derive(Clone)]
pub struct PreparedRequest {
    pub url: String,
    pub headers: RequestHeaders,
    pub body: serde_json::Value,
    /// Echo of the model that was authorized (never silently rewritten).
    pub model: String,
    pub protocol: ProtocolShape,
}

impl fmt::Debug for PreparedRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedRequest")
            .field("url", &self.url)
            .field("headers", &self.headers)
            .field("body", &self.body)
            .field("model", &self.model)
            .field("protocol", &self.protocol)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_headers_debug_redacts_authorization() {
        let marker = format!("{}{}", "hdr-secret-", "ee55ff66");
        let mut headers = RequestHeaders::new();
        headers.insert("Content-Type", "application/json");
        headers.insert("Authorization", format!("Bearer {marker}"));
        let rendered = format!("{headers:?}");
        assert!(
            !rendered.contains(&marker),
            "Authorization value leaked in Debug: {rendered}"
        );
        assert!(rendered.contains("REDACTED") || rendered.contains("[REDACTED]"));
        assert!(rendered.contains("application/json"));
    }
}
