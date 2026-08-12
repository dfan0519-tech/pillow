//! Slot execution-path selection: GrokShell vs NativeAdapter.
//!
//! Path is **derived** from auth + provider request-shape — never a UI toggle.
//! Combinations the Grok shell cannot express are rejected with an explicit
//! error (no silent `base_url`+`env_key` stuffing).

use std::fmt;

use keeper_storage_sqlite::SlotAuth;
use pillow_provider_adapters::{
    AdapterLookupError, AuthInjection, Credential, PrepareError, PreparedRequest, ProtocolShape,
    ProviderAdapter, SecretToken, is_grok_shell_expressible, prepare_request,
    request_adapter_for_provider,
};
use thiserror::Error;

/// How a conversation slot is executed at start time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotExecutionPath {
    /// Existing Grok ACP child: private config `base_url` + bearer `env_key`.
    GrokShell,
    /// Pillow host is the harness; model HTTP uses `pillow-provider-adapters`.
    NativeAdapter,
    /// Not a conversation runtime (e.g. LocalCli status-only).
    NotRunnable,
}

impl SlotExecutionPath {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GrokShell => "grok_shell",
            Self::NativeAdapter => "native_adapter",
            Self::NotRunnable => "not_runnable",
        }
    }
}

/// Governance completeness for an execution path.
///
/// Incomplete paths **must not** be assigned Controller (existing store rule
/// for LocalCli). Full paths keep tool whitelist / write / process on the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathGovernance {
    /// Tools, writes, and process proposals return to the Pillow host.
    Full,
    /// Explicit incomplete governance — not a conversation runtime.
    Incomplete {
        /// Stable reason code (no secrets).
        reason: PathIncompleteReason,
    },
}

impl PathGovernance {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Incomplete { .. } => "incomplete",
        }
    }

    pub fn is_full(self) -> bool {
        matches!(self, Self::Full)
    }

    /// Controller requires full governance — incomplete paths never qualify.
    pub fn may_be_controller(self) -> bool {
        self.is_full()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathIncompleteReason {
    /// Local CLI is status-only; Pillow does not own its tool surface.
    LocalCliUngoverned,
}

impl PathIncompleteReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalCliUngoverned => "local_cli_ungoverned",
        }
    }
}

/// Result of pure path resolution (no secret I/O).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPathDecision {
    pub path: SlotExecutionPath,
    pub governance: PathGovernance,
    /// Provider id used for adapter lookup when applicable.
    pub provider_id: Option<String>,
    /// Adapter descriptor that drove the decision (when known).
    pub adapter: Option<ProviderAdapter>,
    /// Human-readable reason for the chosen path (no secrets).
    pub rationale: &'static str,
}

/// Why path resolution or native prepare refused.
///
/// Display / Debug never embed token material.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExecutionPathError {
    #[error(
        "provider `{provider_id}` requires NativeAdapter (not expressible as grok shell base_url+env_key): {detail}"
    )]
    NotGrokShellExpressible { provider_id: String, detail: String },

    #[error("local CLI slots have incomplete governance and cannot start a conversation runtime")]
    LocalCliNotRunnable,

    #[error("provider adapter lookup failed for `{provider_id}`: {detail}")]
    AdapterLookup { provider_id: String, detail: String },

    #[error("native adapter request prepare failed: {detail}")]
    NativePrepare { detail: String },

    #[error("credential kind mismatch for provider `{provider_id}`")]
    CredentialKindMismatch { provider_id: String },
}

impl ExecutionPathError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotGrokShellExpressible { .. } => "runtime_slot_path_not_grok_shell_expressible",
            Self::LocalCliNotRunnable => "runtime_slot_local_cli_not_runnable",
            Self::AdapterLookup { .. } => "runtime_slot_adapter_lookup_failed",
            Self::NativePrepare { .. } => "runtime_slot_native_prepare_failed",
            Self::CredentialKindMismatch { .. } => "runtime_slot_credential_kind_mismatch",
        }
    }
}

/// Mechanism statement for NativeAdapter full governance (pinned by tests).
///
/// When the path is [`SlotExecutionPath::NativeAdapter`], Pillow **is** the
/// agent harness: model HTTP is prepared by provider-adapters, while tool
/// whitelist, write proposals, and process proposals stay on the host
/// (runtime-host / conversation service) — they do **not** move into a vendor
/// CLI. That is why governance remains `Full` and Controller remains allowed.
pub const NATIVE_ADAPTER_GOVERNANCE_MECHANISM: &str = "host_harness_tools_write_process";

/// Pure path selection from auth. No secret decryption.
pub fn resolve_execution_path(
    auth: &SlotAuth,
) -> Result<ExecutionPathDecision, ExecutionPathError> {
    match auth {
        SlotAuth::GrokLogin { .. } => Ok(ExecutionPathDecision {
            path: SlotExecutionPath::GrokShell,
            governance: PathGovernance::Full,
            provider_id: None,
            adapter: None,
            rationale: "grok_login_uses_user_profile_via_grok_shell",
        }),
        SlotAuth::ApiKey { base_url, .. } => {
            let adapter = request_adapter_for_provider("openai-compatible", Some(base_url))
                .map_err(|e| map_lookup("openai-compatible", e))?;
            // ApiKey slots are always shell-expressible by construction
            // (openai-compatible StandardBearer). If that ever changes, fail closed.
            if !is_grok_shell_expressible(&adapter) {
                return Err(ExecutionPathError::NotGrokShellExpressible {
                    provider_id: String::from("openai-compatible"),
                    detail: String::from(
                        "api_key adapter unexpectedly not shell-expressible; refusing grok shell",
                    ),
                });
            }
            Ok(ExecutionPathDecision {
                path: SlotExecutionPath::GrokShell,
                governance: PathGovernance::Full,
                provider_id: Some(String::from("openai-compatible")),
                adapter: Some(adapter),
                rationale: "api_key_openai_compatible_via_grok_shell",
            })
        }
        SlotAuth::Oauth {
            provider_id,
            base_url,
            ..
        } => resolve_oauth_path(provider_id, base_url),
        SlotAuth::LocalCli { .. } => Ok(ExecutionPathDecision {
            path: SlotExecutionPath::NotRunnable,
            governance: PathGovernance::Incomplete {
                reason: PathIncompleteReason::LocalCliUngoverned,
            },
            provider_id: None,
            adapter: None,
            rationale: "local_cli_status_only_incomplete_governance",
        }),
    }
}

fn resolve_oauth_path(
    provider_id: &str,
    base_url: &str,
) -> Result<ExecutionPathDecision, ExecutionPathError> {
    let adapter = request_adapter_for_provider(provider_id, Some(base_url))
        .map_err(|e| map_lookup(provider_id, e))?;

    if is_grok_shell_expressible(&adapter) {
        Ok(ExecutionPathDecision {
            path: SlotExecutionPath::GrokShell,
            governance: PathGovernance::Full,
            provider_id: Some(provider_id.to_owned()),
            adapter: Some(adapter),
            rationale: "oauth_standard_bearer_openai_compatible_via_grok_shell",
        })
    } else {
        // Requires NativeAdapter — Pillow host is the harness (full governance).
        Ok(ExecutionPathDecision {
            path: SlotExecutionPath::NativeAdapter,
            governance: PathGovernance::Full,
            provider_id: Some(provider_id.to_owned()),
            adapter: Some(adapter),
            rationale: "oauth_requires_request_rewrite_native_adapter_host_harness",
        })
    }
}

fn map_lookup(provider_id: &str, err: AdapterLookupError) -> ExecutionPathError {
    ExecutionPathError::AdapterLookup {
        provider_id: provider_id.to_owned(),
        detail: err.to_string(),
    }
}

/// Prepared launch package for the NativeAdapter path.
///
/// Holds a fully prepared model HTTP request. Secrets live only in
/// [`PreparedRequest`] headers (Debug-redacted) and never include a refresh
/// token — callers inject access material only.
pub struct PreparedNativeAdapter {
    pub provider_id: String,
    pub model_id: String,
    pub path: SlotExecutionPath,
    pub governance: PathGovernance,
    /// Stable mechanism id for tests / audit (see [`NATIVE_ADAPTER_GOVERNANCE_MECHANISM`]).
    pub governance_mechanism: &'static str,
    pub protocol: ProtocolShape,
    pub request: PreparedRequest,
}

impl fmt::Debug for PreparedNativeAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedNativeAdapter")
            .field("provider_id", &self.provider_id)
            .field("model_id", &self.model_id)
            .field("path", &self.path)
            .field("governance", &self.governance)
            .field("governance_mechanism", &self.governance_mechanism)
            .field("protocol", &self.protocol)
            .field("request", &self.request) // PreparedRequest Debug redacts headers
            .finish()
    }
}

/// Build a native prepare package from adapter + already-resolved access credential.
///
/// `body` is the JSON body the host will send (may be a minimal probe body).
/// Refresh tokens must never be passed in `credential`.
pub fn prepare_native_request(
    adapter: &ProviderAdapter,
    credential: &Credential,
    model: &str,
    body: serde_json::Value,
) -> Result<PreparedNativeAdapter, ExecutionPathError> {
    if is_grok_shell_expressible(adapter) {
        // Defensive: native prepare is only for non-shell shapes. Callers that
        // want shell use prepare_grok. Mixing would hide mis-routing.
        return Err(ExecutionPathError::NativePrepare {
            detail: String::from(
                "adapter is grok-shell expressible; refuse native prepare to avoid dual paths",
            ),
        });
    }
    if credential.kind() != adapter.required_credential {
        return Err(ExecutionPathError::CredentialKindMismatch {
            provider_id: adapter.provider_id.clone(),
        });
    }

    let request = prepare_request(adapter, credential, model, body).map_err(|e| {
        // Map prepare errors without embedding token material (PrepareError is clean).
        ExecutionPathError::NativePrepare {
            detail: prepare_error_detail(e),
        }
    })?;

    Ok(PreparedNativeAdapter {
        provider_id: adapter.provider_id.clone(),
        model_id: model.to_owned(),
        path: SlotExecutionPath::NativeAdapter,
        governance: PathGovernance::Full,
        governance_mechanism: NATIVE_ADAPTER_GOVERNANCE_MECHANISM,
        protocol: request.protocol,
        request,
    })
}

fn prepare_error_detail(err: PrepareError) -> String {
    // PrepareError Display is structural only (no tokens).
    err.to_string()
}

/// Build an OAuth credential from access token material only (no refresh).
pub fn oauth_access_credential(
    access_token: impl Into<String>,
    account_id: Option<String>,
) -> Credential {
    Credential::OAuth {
        access_token: SecretToken::new(access_token.into()),
        account_id,
        id_token: None,
    }
}

/// Build an API-key credential from secret material.
pub fn api_key_credential(token: impl Into<String>) -> Credential {
    Credential::ApiKey {
        token: SecretToken::new(token.into()),
    }
}

/// Explain why a shell-expressibility failure happened (for error detail).
pub fn shell_inexpressible_detail(adapter: &ProviderAdapter) -> String {
    let mut parts = Vec::new();
    if !matches!(adapter.auth, AuthInjection::StandardBearer) {
        parts.push("auth is not StandardBearer");
    }
    if !adapter.derived_headers.is_empty() {
        parts.push("requires derived headers");
    }
    if !matches!(adapter.protocol, ProtocolShape::OpenAiChatCompletions) {
        parts.push("protocol is not OpenAiChatCompletions");
    }
    if !adapter
        .endpoint_url
        .trim_end_matches('/')
        .ends_with("/chat/completions")
    {
        parts.push("endpoint is not /chat/completions");
    }
    if parts.is_empty() {
        String::from("adapter marked not shell-expressible")
    } else {
        parts.join("; ")
    }
}

/// Assert that forcing GrokShell for a non-expressible adapter is rejected.
///
/// Used by start/prepare paths that might otherwise fall through to
/// `prepare_grok`.
pub fn reject_if_not_shell_expressible(
    adapter: &ProviderAdapter,
) -> Result<(), ExecutionPathError> {
    if is_grok_shell_expressible(adapter) {
        return Ok(());
    }
    Err(ExecutionPathError::NotGrokShellExpressible {
        provider_id: adapter.provider_id.clone(),
        detail: shell_inexpressible_detail(adapter),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    // Imported here, not at module scope: only tests construct these, and
    // clippy does not compile test code, so a module-level import reads as
    // unused and fails -D warnings while the tests still need it.
    use pillow_provider_adapters::CredentialKind;
    use pillow_provider_adapters::{chatgpt_codex_subscription, openai_compatible};
    use serde_json::json;

    fn oauth_auth(provider_id: &str, base_url: &str) -> SlotAuth {
        SlotAuth::Oauth {
            provider_id: provider_id.into(),
            issuer: "https://issuer.example.invalid".into(),
            client_id: "client".into(),
            scopes: vec!["openid".into()],
            token_endpoint: "https://issuer.example.invalid/token".into(),
            account_label: Some("acct-label".into()),
            base_url: base_url.into(),
            model_id: "gpt-5.4".into(),
            access_secret_ref: "00000000-0000-0000-0000-000000000001".into(),
            refresh_secret_ref: "00000000-0000-0000-0000-000000000002".into(),
            access_expires_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn grok_login_is_grok_shell_full() {
        let d = resolve_execution_path(&SlotAuth::GrokLogin { model_id: None }).unwrap();
        assert_eq!(d.path, SlotExecutionPath::GrokShell);
        assert!(d.governance.is_full());
        assert!(d.governance.may_be_controller());
    }

    #[test]
    fn api_key_is_grok_shell_full() {
        let d = resolve_execution_path(&SlotAuth::ApiKey {
            base_url: "https://api.example.invalid/v1".into(),
            model_id: "m".into(),
            secret_ref: "00000000-0000-0000-0000-000000000099".into(),
        })
        .unwrap();
        assert_eq!(d.path, SlotExecutionPath::GrokShell);
        assert!(d.governance.is_full());
    }

    #[test]
    fn standard_oauth_is_grok_shell() {
        let d = resolve_execution_path(&oauth_auth(
            "example-oidc",
            "https://api.example.invalid/v1",
        ))
        .unwrap();
        assert_eq!(d.path, SlotExecutionPath::GrokShell);
        assert!(d.governance.is_full());
    }

    #[test]
    fn chatgpt_codex_oauth_is_native_adapter_full() {
        let d = resolve_execution_path(&oauth_auth(
            "chatgpt-codex",
            "https://api.example.invalid/v1",
        ))
        .unwrap();
        assert_eq!(d.path, SlotExecutionPath::NativeAdapter);
        assert_eq!(d.governance, PathGovernance::Full);
        assert!(d.governance.may_be_controller());
        assert_eq!(d.provider_id.as_deref(), Some("chatgpt-codex"));
        let adapter = d.adapter.as_ref().expect("adapter");
        assert!(!is_grok_shell_expressible(adapter));
    }

    #[test]
    fn local_cli_is_incomplete_and_not_controller() {
        let d = resolve_execution_path(&SlotAuth::LocalCli {
            executable_path: "C:\\tools\\cli.exe".into(),
            launch_args: vec![],
            status_args: vec!["--version".into()],
        })
        .unwrap();
        assert!(!d.governance.is_full());
        assert!(!d.governance.may_be_controller());
        assert!(matches!(
            d.governance,
            PathGovernance::Incomplete {
                reason: PathIncompleteReason::LocalCliUngoverned
            }
        ));
    }

    #[test]
    fn reject_stuffing_codex_into_grok_shell() {
        let adapter = chatgpt_codex_subscription();
        let err = reject_if_not_shell_expressible(&adapter).expect_err("must reject");
        assert_eq!(err.code(), "runtime_slot_path_not_grok_shell_expressible");
        let text = format!("{err}");
        assert!(text.contains("chatgpt-codex"));
        assert!(!text.contains("Bearer "));
    }

    #[test]
    fn native_prepare_uses_access_only_and_redacts_debug() {
        let adapter = chatgpt_codex_subscription();
        let access = format!("{}{}", "native-acc-", "ess-token-zz01");
        let refresh = format!("{}{}", "native-ref-", "resh-must-not-appear-zz02");
        let cred = oauth_access_credential(access.clone(), Some("acct-xyz".into()));
        let prepared = prepare_native_request(&adapter, &cred, "gpt-5.4", json!({"input": []}))
            .expect("prepare");
        assert_eq!(prepared.path, SlotExecutionPath::NativeAdapter);
        assert_eq!(
            prepared.governance_mechanism,
            NATIVE_ADAPTER_GOVERNANCE_MECHANISM
        );
        assert!(prepared.governance.is_full());
        assert_eq!(
            prepared.request.headers.get("Authorization"),
            Some(format!("Bearer {access}").as_str())
        );
        assert_eq!(
            prepared.request.headers.get("ChatGPT-Account-Id"),
            Some("acct-xyz")
        );

        let debug = format!("{prepared:?}");
        assert!(!debug.contains(&access), "access leaked in Debug: {debug}");
        assert!(
            !debug.contains(&refresh),
            "refresh marker must never appear: {debug}"
        );
        assert!(debug.contains("REDACTED") || debug.contains("[REDACTED]"));
    }

    #[test]
    fn native_prepare_refuses_shell_expressible_adapter() {
        let adapter = openai_compatible("https://api.example.test/v1").unwrap();
        let cred = api_key_credential(format!("{}{}", "key-", "shell-only"));
        let err = prepare_native_request(&adapter, &cred, "m", json!({})).expect_err("refuse");
        assert_eq!(err.code(), "runtime_slot_native_prepare_failed");
    }

    #[test]
    fn credential_kind_mismatch_on_native_prepare() {
        let adapter = chatgpt_codex_subscription();
        let cred = api_key_credential(format!("{}{}", "key-", "wrong-kind"));
        let err = prepare_native_request(&adapter, &cred, "gpt-5.4", json!({})).expect_err("kind");
        assert_eq!(err.code(), "runtime_slot_credential_kind_mismatch");
        assert_eq!(adapter.required_credential, CredentialKind::OAuth);
    }
}
