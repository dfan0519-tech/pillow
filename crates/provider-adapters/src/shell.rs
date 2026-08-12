//! Classify whether a provider request shape fits the Grok shell
//! (`base_url` + standard bearer `env_key`) or requires the NativeAdapter path.

use crate::adapters::{chatgpt_codex_subscription, openai_compatible};
use crate::prepare::PrepareError;
use crate::types::{AuthInjection, ProtocolShape, ProviderAdapter};

/// Why a provider id could not be mapped to a request adapter.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdapterLookupError {
    #[error("provider `{provider_id}` requires a base_url for OpenAI-compatible shape")]
    BaseUrlRequired { provider_id: String },

    #[error("provider `{provider_id}` has an empty or invalid base_url")]
    InvalidBaseUrl { provider_id: String },

    #[error("provider `{provider_id}` is unknown and has no base_url fallback")]
    UnknownProvider { provider_id: String },
}

/// Whether the Grok shell can express this adapter without silent loss.
///
/// The shell only supports:
/// - standard `Authorization: Bearer <env_key>`
/// - OpenAI-compatible chat completions at `{base_url}/chat/completions`
/// - no derived headers (e.g. `ChatGPT-Account-Id`)
///
/// Anything else **must** use the NativeAdapter path. Stuffing a non-expressible
/// shape into `base_url`+`env_key` is a hard reject at the runtime-slots layer.
pub fn is_grok_shell_expressible(adapter: &ProviderAdapter) -> bool {
    if !matches!(adapter.auth, AuthInjection::StandardBearer) {
        return false;
    }
    if !adapter.derived_headers.is_empty() {
        return false;
    }
    if !matches!(adapter.protocol, ProtocolShape::OpenAiChatCompletions) {
        return false;
    }
    // Endpoint must be the standard chat-completions suffix so the shell's
    // base_url rewrite stays correct.
    let url = adapter.endpoint_url.trim_end_matches('/');
    url.ends_with("/chat/completions")
}

/// Resolve the pure request-shape adapter for a provider id.
///
/// - `chatgpt-codex` → fixed Codex subscription shape (never shell-expressible).
/// - Known OpenAI-compatible login ids (`openai`, `xai`, `custom-oidc`, …)
///   and unknown ids **with** `base_url` → [`openai_compatible`].
/// - Unknown id without `base_url` → reject.
pub fn request_adapter_for_provider(
    provider_id: &str,
    base_url: Option<&str>,
) -> Result<ProviderAdapter, AdapterLookupError> {
    let id = provider_id.trim();
    if id.is_empty() {
        return Err(AdapterLookupError::UnknownProvider {
            provider_id: provider_id.to_owned(),
        });
    }

    if id == "chatgpt-codex" {
        return Ok(chatgpt_codex_subscription());
    }

    // Standard OIDC / OpenAI-compatible providers need a user-supplied base_url.
    let base = match base_url.map(str::trim).filter(|s| !s.is_empty()) {
        Some(b) => b,
        None => {
            // Known ids still require base_url for the request shape.
            if is_known_openai_compatible_provider(id) {
                return Err(AdapterLookupError::BaseUrlRequired {
                    provider_id: id.to_owned(),
                });
            }
            return Err(AdapterLookupError::UnknownProvider {
                provider_id: id.to_owned(),
            });
        }
    };

    openai_compatible(base).map_err(|err| match err {
        PrepareError::EmptyEndpoint { .. } => AdapterLookupError::InvalidBaseUrl {
            provider_id: id.to_owned(),
        },
        _ => AdapterLookupError::InvalidBaseUrl {
            provider_id: id.to_owned(),
        },
    })
}

fn is_known_openai_compatible_provider(provider_id: &str) -> bool {
    matches!(
        provider_id,
        "openai" | "openai-compatible" | "xai" | "custom-oidc" | "example-oidc" | "github-copilot"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_compatible_is_shell_expressible() {
        let a = openai_compatible("https://api.example.test/v1").expect("ok");
        assert!(is_grok_shell_expressible(&a));
    }

    #[test]
    fn chatgpt_codex_is_not_shell_expressible() {
        let a = chatgpt_codex_subscription();
        assert!(!is_grok_shell_expressible(&a));
        assert!(matches!(
            a.auth,
            AuthInjection::CustomBearer {
                strip_standard_authorization: true,
                ..
            }
        ));
        assert!(!a.derived_headers.is_empty());
    }

    #[test]
    fn lookup_codex_ignores_base_url() {
        let a = request_adapter_for_provider("chatgpt-codex", Some("https://ignored.example/v1"))
            .expect("codex");
        assert_eq!(a.provider_id, "chatgpt-codex");
        assert!(!is_grok_shell_expressible(&a));
    }

    #[test]
    fn lookup_openai_requires_base_url() {
        let err = request_adapter_for_provider("openai", None).expect_err("need base");
        assert!(matches!(err, AdapterLookupError::BaseUrlRequired { .. }));
    }

    #[test]
    fn lookup_unknown_without_base_rejects() {
        let err = request_adapter_for_provider("totally-unknown-vendor", None).expect_err("unk");
        assert!(matches!(err, AdapterLookupError::UnknownProvider { .. }));
    }

    #[test]
    fn lookup_unknown_with_base_uses_openai_compatible() {
        let a = request_adapter_for_provider(
            "totally-unknown-vendor",
            Some("https://proxy.example.test/v1"),
        )
        .expect("fallback");
        assert!(is_grok_shell_expressible(&a));
        assert!(a.endpoint_url.ends_with("/chat/completions"));
    }
}
