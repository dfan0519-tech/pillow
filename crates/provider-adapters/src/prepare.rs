//! Pure request preparation: adapter + credential + model + body → headers/url.

use thiserror::Error;

use crate::credential::{Credential, CredentialKind};
use crate::jwt::resolve_chatgpt_account_id;
use crate::types::{
    AuthInjection, DerivedHeader, PreparedRequest, ProviderAdapter, RequestHeaders,
};

/// Why [`prepare_request`] refused to build a request.
///
/// Display / Debug never include token material — only structural reasons.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PrepareError {
    #[error("model `{model}` is not in the allowlist for provider `{provider_id}`")]
    ModelNotAllowed { provider_id: String, model: String },

    #[error(
        "credential kind mismatch for provider `{provider_id}`: required {required:?}, got {provided:?}"
    )]
    CredentialKindMismatch {
        provider_id: String,
        required: CredentialKind,
        provided: CredentialKind,
    },

    #[error("derived header `{header_name}` unavailable for provider `{provider_id}`: {reason}")]
    DerivedHeaderUnavailable {
        provider_id: String,
        header_name: String,
        reason: String,
    },

    #[error("provider `{provider_id}` has an empty endpoint URL")]
    EmptyEndpoint { provider_id: String },

    #[error("model id must be non-empty")]
    EmptyModel,

    #[error("credential access material must be non-empty")]
    EmptyCredential,
}

/// Build a [`PreparedRequest`] or return an explicit rejection.
///
/// Guarantees:
/// - Model not on allowlist → **reject** (never substitute another model).
/// - Credential kind mismatch → **reject**.
/// - Required derived header cannot be resolved → **reject** (never send
///   a request missing that header).
/// - No network I/O.
pub fn prepare_request(
    adapter: &ProviderAdapter,
    credential: &Credential,
    model: &str,
    body: serde_json::Value,
) -> Result<PreparedRequest, PrepareError> {
    if model.trim().is_empty() {
        return Err(PrepareError::EmptyModel);
    }
    if adapter.endpoint_url.trim().is_empty() {
        return Err(PrepareError::EmptyEndpoint {
            provider_id: adapter.provider_id.clone(),
        });
    }
    if credential.access_material().is_empty() {
        return Err(PrepareError::EmptyCredential);
    }

    // Allowlist: empty means unrestricted; non-empty is exact-id match only.
    if !adapter.model_allowlist.is_empty() && !adapter.model_allowlist.iter().any(|m| m == model) {
        return Err(PrepareError::ModelNotAllowed {
            provider_id: adapter.provider_id.clone(),
            model: model.to_owned(),
        });
    }

    let provided = credential.kind();
    if provided != adapter.required_credential {
        return Err(PrepareError::CredentialKindMismatch {
            provider_id: adapter.provider_id.clone(),
            required: adapter.required_credential,
            provided,
        });
    }

    let mut headers = RequestHeaders::new();
    for (name, value) in &adapter.constant_headers {
        headers.insert(name.clone(), value.clone());
    }

    match &adapter.auth {
        AuthInjection::StandardBearer => {
            let token = credential.access_material();
            headers.insert("Authorization", format!("Bearer {token}"));
        }
        AuthInjection::CustomBearer {
            header_name,
            strip_standard_authorization,
        } => {
            if *strip_standard_authorization {
                headers.remove_authorization();
            }
            let token = credential.access_material();
            headers.insert(header_name.clone(), format!("Bearer {token}"));
        }
    }

    for derived in &adapter.derived_headers {
        match derived {
            DerivedHeader::ChatGptAccountId { header_name } => {
                let account_id = resolve_account_id(credential).ok_or_else(|| {
                    PrepareError::DerivedHeaderUnavailable {
                        provider_id: adapter.provider_id.clone(),
                        header_name: header_name.clone(),
                        reason: String::from(
                            "ChatGPT account id missing from credential and JWT claims",
                        ),
                    }
                })?;
                headers.insert(header_name.clone(), account_id);
            }
        }
    }

    Ok(PreparedRequest {
        url: adapter.endpoint_url.clone(),
        headers,
        body,
        model: model.to_owned(),
        protocol: adapter.protocol,
    })
}

fn resolve_account_id(credential: &Credential) -> Option<String> {
    match credential {
        Credential::ApiKey { .. } => None,
        Credential::OAuth {
            access_token,
            account_id,
            id_token,
        } => resolve_chatgpt_account_id(
            account_id.as_deref(),
            id_token.as_ref().map(|t| t.as_str()),
            access_token.as_str(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{chatgpt_codex_subscription, openai_compatible};
    use crate::credential::SecretToken;
    use serde_json::json;

    fn api_key_cred(token: &str) -> Credential {
        Credential::ApiKey {
            token: SecretToken::new(token.to_owned()),
        }
    }

    fn oauth_cred(access: &str, account_id: Option<&str>) -> Credential {
        Credential::OAuth {
            access_token: SecretToken::new(access.to_owned()),
            account_id: account_id.map(str::to_owned),
            id_token: None,
        }
    }

    #[test]
    fn openai_compatible_sets_bearer_and_url() {
        let adapter = openai_compatible("https://api.example.test/v1").expect("adapter");
        let marker = format!("{}{}", "prep-key-", "ii99jj00");
        let prepared = prepare_request(
            &adapter,
            &api_key_cred(&marker),
            "gpt-test",
            json!({"messages": []}),
        )
        .expect("prepare");
        assert_eq!(prepared.url, "https://api.example.test/v1/chat/completions");
        assert_eq!(
            prepared.headers.get("Authorization"),
            Some(format!("Bearer {marker}").as_str())
        );
        assert_eq!(prepared.model, "gpt-test");
    }

    #[test]
    fn model_not_on_allowlist_is_rejected_not_substituted() {
        let adapter = chatgpt_codex_subscription();
        let err = prepare_request(
            &adapter,
            &oauth_cred("access-token-material", Some("acct-1")),
            "gpt-4o-not-allowed",
            json!({}),
        )
        .expect_err("must reject");
        match err {
            PrepareError::ModelNotAllowed { model, .. } => {
                assert_eq!(model, "gpt-4o-not-allowed");
            }
            other => panic!("wrong error: {other:?}"),
        }
        // Must not succeed with a different model.
        assert!(
            !adapter
                .model_allowlist
                .iter()
                .any(|m| m == "gpt-4o-not-allowed")
        );
    }

    #[test]
    fn credential_kind_mismatch_rejected() {
        let adapter = chatgpt_codex_subscription();
        let err = prepare_request(
            &adapter,
            &api_key_cred("api-key-material"),
            "gpt-5.4",
            json!({}),
        )
        .expect_err("must reject");
        match err {
            PrepareError::CredentialKindMismatch {
                required, provided, ..
            } => {
                assert_eq!(required, CredentialKind::OAuth);
                assert_eq!(provided, CredentialKind::ApiKey);
            }
            other => panic!("wrong error: {other:?}"),
        }
    }

    #[test]
    fn missing_derived_account_id_rejected() {
        let adapter = chatgpt_codex_subscription();
        // Opaque non-JWT access token, no explicit account_id, no id_token.
        let err = prepare_request(
            &adapter,
            &oauth_cred("opaque-access-not-a-jwt", None),
            "gpt-5.4",
            json!({}),
        )
        .expect_err("must reject");
        match err {
            PrepareError::DerivedHeaderUnavailable { header_name, .. } => {
                assert_eq!(header_name, "ChatGPT-Account-Id");
            }
            other => panic!("wrong error: {other:?}"),
        }
    }

    #[test]
    fn codex_success_strips_and_sets_headers() {
        let adapter = chatgpt_codex_subscription();
        let access = format!("{}{}", "codex-access-", "kk11ll22");
        let prepared = prepare_request(
            &adapter,
            &oauth_cred(&access, Some("acct-xyz")),
            "gpt-5.4",
            json!({"input": []}),
        )
        .expect("prepare");
        assert_eq!(
            prepared.url,
            "https://chatgpt.com/backend-api/codex/responses"
        );
        assert_eq!(
            prepared.headers.get("Authorization"),
            Some(format!("Bearer {access}").as_str())
        );
        assert_eq!(prepared.headers.get("ChatGPT-Account-Id"), Some("acct-xyz"));
        assert_eq!(prepared.protocol, crate::ProtocolShape::OpenAiResponses);
    }

    #[test]
    fn prepare_error_display_has_no_token() {
        let marker = format!("{}{}", "err-token-", "mm33nn44");
        let adapter = chatgpt_codex_subscription();
        let err = prepare_request(&adapter, &oauth_cred(&marker, None), "gpt-5.4", json!({}))
            .expect_err("reject");
        let text = format!("{err}");
        let debug = format!("{err:?}");
        assert!(!text.contains(&marker), "Display leaked token: {text}");
        assert!(!debug.contains(&marker), "Debug leaked token: {debug}");
    }
}
