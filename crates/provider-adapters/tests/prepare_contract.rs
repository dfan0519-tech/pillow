//! Contract tests for prepare_request rejections and Debug redaction.
//!
//! Fake tokens are assembled at runtime — no secret-shaped literals in source.

use pillow_provider_adapters::{
    Credential, CredentialKind, PrepareError, SecretToken, chatgpt_codex_subscription,
    openai_compatible, prepare_request,
};
use serde_json::json;

fn runtime_token(prefix: &str, suffix: &str) -> String {
    format!("{prefix}{suffix}")
}

fn api_key(token: &str) -> Credential {
    Credential::ApiKey {
        token: SecretToken::new(token.to_owned()),
    }
}

fn oauth(access: &str, account_id: Option<&str>) -> Credential {
    Credential::OAuth {
        access_token: SecretToken::new(access.to_owned()),
        account_id: account_id.map(str::to_owned),
        id_token: None,
    }
}

// --- Required rejection cases ---

#[test]
fn rejects_model_not_on_allowlist_without_substitution() {
    let adapter = chatgpt_codex_subscription();
    let access = runtime_token("reject-model-", "access-aa01");
    let disallowed = runtime_token("gpt-not-on-", "list-zz99");
    assert!(
        !adapter.model_allowlist.iter().any(|m| m == &disallowed),
        "test model accidentally allowlisted"
    );

    let err = prepare_request(
        &adapter,
        &oauth(&access, Some("acct-ok")),
        &disallowed,
        json!({}),
    )
    .expect_err("model outside allowlist must be rejected");

    match &err {
        PrepareError::ModelNotAllowed { model, provider_id } => {
            assert_eq!(model, &disallowed);
            assert_eq!(provider_id, "chatgpt-codex");
        }
        other => panic!("expected ModelNotAllowed, got {other:?}"),
    }

    // No silent swap: a allowed model must still work under the same credential.
    let ok = prepare_request(
        &adapter,
        &oauth(&access, Some("acct-ok")),
        "gpt-5.4",
        json!({}),
    )
    .expect("allowlisted model must pass");
    assert_eq!(ok.model, "gpt-5.4");
}

#[test]
fn rejects_credential_kind_mismatch() {
    let adapter = chatgpt_codex_subscription();
    let key = runtime_token("reject-kind-", "key-bb02");

    let err = prepare_request(&adapter, &api_key(&key), "gpt-5.4", json!({}))
        .expect_err("api key must not satisfy OAuth adapter");

    match err {
        PrepareError::CredentialKindMismatch {
            required, provided, ..
        } => {
            assert_eq!(required, CredentialKind::OAuth);
            assert_eq!(provided, CredentialKind::ApiKey);
        }
        other => panic!("expected CredentialKindMismatch, got {other:?}"),
    }

    // Symmetric: OpenAI-compatible requires ApiKey, rejects OAuth.
    let openai = openai_compatible("https://api.example.test/v1").expect("adapter");
    let err2 = prepare_request(&openai, &oauth(&key, Some("acct")), "any-model", json!({}))
        .expect_err("oauth must not satisfy api-key adapter");
    match err2 {
        PrepareError::CredentialKindMismatch {
            required, provided, ..
        } => {
            assert_eq!(required, CredentialKind::ApiKey);
            assert_eq!(provided, CredentialKind::OAuth);
        }
        other => panic!("expected CredentialKindMismatch, got {other:?}"),
    }
}

#[test]
fn rejects_when_derived_chatgpt_account_id_unavailable() {
    let adapter = chatgpt_codex_subscription();
    // Opaque non-JWT access token: no account_id field, no id_token claims.
    let access = runtime_token("reject-derived-", "opaque-cc03");

    let err = prepare_request(&adapter, &oauth(&access, None), "gpt-5.4", json!({}))
        .expect_err("missing account id must reject, not send incomplete headers");

    match err {
        PrepareError::DerivedHeaderUnavailable { header_name, .. } => {
            assert_eq!(header_name, "ChatGPT-Account-Id");
        }
        other => panic!("expected DerivedHeaderUnavailable, got {other:?}"),
    }
}

// --- Debug / error redaction ---

#[test]
fn prepared_request_debug_never_contains_token() {
    let adapter = openai_compatible("https://api.example.test/v1").expect("adapter");
    let token = runtime_token("debug-redact-", "token-dd04-SECRET");
    let prepared = prepare_request(
        &adapter,
        &api_key(&token),
        "model-x",
        json!({"stream": false}),
    )
    .expect("prepare");

    let rendered = format!("{prepared:?}");
    assert!(
        !rendered.contains(&token),
        "PreparedRequest Debug must not contain token; got: {rendered}"
    );
    assert!(
        !rendered.contains(&format!("Bearer {token}")),
        "Bearer+token must not appear in Debug"
    );
    // Non-secret fields remain visible for diagnostics.
    assert!(rendered.contains("https://api.example.test/v1/chat/completions"));
    assert!(rendered.contains("model-x"));
}

#[test]
fn codex_prepared_debug_never_contains_access_token() {
    let adapter = chatgpt_codex_subscription();
    let access = runtime_token("codex-debug-", "access-ee05-SECRET");
    let prepared = prepare_request(
        &adapter,
        &oauth(&access, Some("acct-public-label")),
        "gpt-5.5",
        json!({"input": []}),
    )
    .expect("prepare");

    let rendered = format!("{prepared:?}");
    assert!(
        !rendered.contains(&access),
        "access token in Debug: {rendered}"
    );
    // Account id is not a token; may appear (non-secret label).
    assert!(rendered.contains("acct-public-label") || rendered.contains("ChatGPT-Account-Id"));
}

#[test]
fn prepare_error_messages_never_contain_token() {
    let adapter = chatgpt_codex_subscription();
    let access = runtime_token("err-surface-", "token-ff06-SECRET");

    let cases: Vec<PrepareError> = vec![
        prepare_request(&adapter, &oauth(&access, None), "gpt-5.4", json!({}))
            .expect_err("derived"),
        prepare_request(&adapter, &api_key(&access), "gpt-5.4", json!({})).expect_err("kind"),
        prepare_request(
            &adapter,
            &oauth(&access, Some("acct")),
            "totally-unknown-model",
            json!({}),
        )
        .expect_err("model"),
    ];

    for err in cases {
        let display = format!("{err}");
        let debug = format!("{err:?}");
        assert!(
            !display.contains(&access),
            "Display leaked token: {display}"
        );
        assert!(!debug.contains(&access), "Debug leaked token: {debug}");
    }
}

#[test]
fn secret_token_and_credential_debug_redacted() {
    let token = runtime_token("type-debug-", "gg07-SECRET");
    let secret = SecretToken::new(token.clone());
    assert!(!format!("{secret:?}").contains(&token));

    let cred = api_key(&token);
    assert!(!format!("{cred:?}").contains(&token));
}

#[test]
fn openai_compatible_happy_path_headers() {
    let adapter = openai_compatible("https://api.openai.com/v1").expect("adapter");
    let token = runtime_token("happy-", "hh08");
    let prepared = prepare_request(
        &adapter,
        &api_key(&token),
        "gpt-4o-mini",
        json!({"messages": [{"role": "user", "content": "hi"}]}),
    )
    .expect("ok");
    assert_eq!(prepared.url, "https://api.openai.com/v1/chat/completions");
    assert_eq!(
        prepared.headers.get("Authorization"),
        Some(format!("Bearer {token}").as_str())
    );
    assert_eq!(
        prepared.headers.get("Content-Type"),
        Some("application/json")
    );
}
