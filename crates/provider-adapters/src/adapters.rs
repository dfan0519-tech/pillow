//! Built-in adapter descriptions with cited endpoint / header shapes.

use crate::credential::CredentialKind;
use crate::prepare::PrepareError;
use crate::types::{AuthInjection, DerivedHeader, ProtocolShape, ProviderAdapter};

/// Codex backend endpoint used by OpenCode’s ChatGPT OAuth path.
///
/// Source: OpenCode MIT `packages/opencode/src/plugin/openai/codex.ts` line 12
/// (`const CODEX_API_ENDPOINT = "https://chatgpt.com/backend-api/codex/responses"`),
/// fixed reference commit `d470434`.
pub const CHATGPT_CODEX_ENDPOINT: &str = "https://chatgpt.com/backend-api/codex/responses";

/// Static subset of models OpenCode allows for ChatGPT OAuth sessions.
///
/// Source: OpenCode MIT `packages/opencode/src/plugin/openai/codex.ts` line 15
/// (`ALLOWED_MODELS`). OpenCode also applies dynamic version filters in
/// `provider.models` (lines 280–317); those are **not** fully encoded here —
/// see residual risk in the execution report.
pub const CHATGPT_CODEX_MODEL_ALLOWLIST: &[&str] =
    &["gpt-5.5", "gpt-5.3-codex-spark", "gpt-5.4", "gpt-5.4-mini"];

/// Standard OpenAI-compatible Chat Completions adapter.
///
/// - **Endpoint:** `{base_url}/chat/completions` where `base_url` is typically
///   `https://api.openai.com/v1` or a compatible proxy root.
/// - **Auth:** `Authorization: Bearer <api_key>`.
/// - **Protocol:** [`ProtocolShape::OpenAiChatCompletions`].
/// - **Allowlist:** empty (unrestricted).
///
/// **Source:** OpenAI API reference — Chat Completions create  
/// <https://platform.openai.com/docs/api-reference/chat/create>  
/// (`POST https://api.openai.com/v1/chat/completions`, Bearer auth).
pub fn openai_compatible(base_url: &str) -> Result<ProviderAdapter, PrepareError> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(PrepareError::EmptyEndpoint {
            provider_id: String::from("openai-compatible"),
        });
    }
    let endpoint_url = format!("{base}/chat/completions");
    Ok(ProviderAdapter {
        provider_id: String::from("openai-compatible"),
        display_name: String::from("OpenAI-compatible"),
        endpoint_url,
        auth: AuthInjection::StandardBearer,
        constant_headers: vec![(
            String::from("Content-Type"),
            String::from("application/json"),
        )],
        derived_headers: Vec::new(),
        model_allowlist: Vec::new(),
        protocol: ProtocolShape::OpenAiChatCompletions,
        required_credential: CredentialKind::ApiKey,
    })
}

/// ChatGPT consumer-subscription path via the proprietary Codex responses backend.
///
/// This is the shape OpenCode uses so Plus/Pro sessions can call models without
/// a standard API key — **not** expressible as bare `base_url` + `env_key`.
///
/// | Field | Value | Source |
/// | --- | --- | --- |
/// | endpoint | [`CHATGPT_CODEX_ENDPOINT`] | `codex.ts:12` |
/// | strip `Authorization` then set Bearer access | rewrite fetch | `codex.ts:343–405` |
/// | `ChatGPT-Account-Id` | derived | `codex.ts:406–408` |
/// | model allowlist | [`CHATGPT_CODEX_MODEL_ALLOWLIST`] | `codex.ts:15` |
/// | protocol | OpenAI Responses shape | endpoint path `/codex/responses` |
///
/// **Not included:** OAuth `client_id`, issuer URLs, or refresh logic (out of
/// scope; no secrets hardcoded). Account id claim extraction mirrors
/// `extractAccountIdFromClaims` (`codex.ts:57–63`) in `crate::jwt`.
pub fn chatgpt_codex_subscription() -> ProviderAdapter {
    ProviderAdapter {
        provider_id: String::from("chatgpt-codex"),
        display_name: String::from("ChatGPT (Codex backend)"),
        endpoint_url: CHATGPT_CODEX_ENDPOINT.to_owned(),
        auth: AuthInjection::CustomBearer {
            header_name: String::from("Authorization"),
            strip_standard_authorization: true,
        },
        constant_headers: vec![(
            String::from("Content-Type"),
            String::from("application/json"),
        )],
        derived_headers: vec![DerivedHeader::ChatGptAccountId {
            header_name: String::from("ChatGPT-Account-Id"),
        }],
        model_allowlist: CHATGPT_CODEX_MODEL_ALLOWLIST
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        protocol: ProtocolShape::OpenAiResponses,
        required_credential: CredentialKind::OAuth,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_compatible_rejects_empty_base() {
        assert!(matches!(
            openai_compatible("  "),
            Err(PrepareError::EmptyEndpoint { .. })
        ));
    }

    #[test]
    fn openai_compatible_joins_path() {
        let a = openai_compatible("https://api.openai.com/v1/").expect("ok");
        assert_eq!(a.endpoint_url, "https://api.openai.com/v1/chat/completions");
        assert!(a.model_allowlist.is_empty());
        assert_eq!(a.required_credential, CredentialKind::ApiKey);
        assert_eq!(a.auth, AuthInjection::StandardBearer);
    }

    #[test]
    fn codex_adapter_shape_matches_reference_constants() {
        let a = chatgpt_codex_subscription();
        assert_eq!(a.endpoint_url, CHATGPT_CODEX_ENDPOINT);
        assert_eq!(a.model_allowlist.len(), CHATGPT_CODEX_MODEL_ALLOWLIST.len());
        assert_eq!(a.required_credential, CredentialKind::OAuth);
        assert!(matches!(
            a.auth,
            AuthInjection::CustomBearer {
                strip_standard_authorization: true,
                ..
            }
        ));
        assert_eq!(a.derived_headers.len(), 1);
    }
}
