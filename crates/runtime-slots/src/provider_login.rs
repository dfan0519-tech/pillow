//! Auditable provider login metadata.
//!
//! This module contains public OAuth/OIDC configuration only. It never carries
//! API keys, authorization codes, access tokens, refresh tokens, or client
//! secrets.

use thiserror::Error;

/// Login mechanism exposed by a provider profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderLoginMethod {
    Pkce,
    DeviceCode,
    ApiKeyOnly,
}

/// Provenance classification for an OAuth client identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientIdSource {
    /// Official provider documentation explicitly makes the client ID
    /// available for third-party clients.
    PublicDocumented,
    /// Pillow requires the user to enter an identifier for an app they own.
    UserSupplied,
    /// No trustworthy third-party-use provenance was found. No value is stored.
    Unknown,
}

/// Whether a profile field is fixed, supplied at runtime, unknown, or unused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileField<T> {
    Known(T),
    UserSupplied,
    Unknown,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientIdProfile {
    pub source: ClientIdSource,
    pub value: ProfileField<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountTokenSource {
    IdToken,
    AccessToken,
}

/// How a successful login can be associated with a stable account label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountIdentifierStrategy {
    TokenClaims {
        token_order: &'static [AccountTokenSource],
        claim_paths: &'static [&'static str],
    },
    UserSupplied,
    Unknown,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderLoginField {
    ProviderId,
    DisplayName,
    LoginMethod,
    Issuer,
    AuthorizeEndpoint,
    DeviceAuthorizationEndpoint,
    TokenEndpoint,
    Scopes,
    ClientId,
    AccountIdentifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderLoginEvidence {
    pub fields: &'static [ProviderLoginField],
    pub source: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderLoginProfile {
    pub provider_id: &'static str,
    pub display_name: &'static str,
    pub login_method: ProviderLoginMethod,
    pub issuer: ProfileField<&'static str>,
    pub authorize_endpoint: ProfileField<&'static str>,
    pub device_authorization_endpoint: ProfileField<&'static str>,
    pub token_endpoint: ProfileField<&'static str>,
    pub scopes: ProfileField<&'static [&'static str]>,
    pub client_id: ClientIdProfile,
    pub account_identifier: AccountIdentifierStrategy,
    pub evidence: &'static [ProviderLoginEvidence],
}

/// Public, non-secret values that may complete an incomplete profile.
///
/// This is deliberately not an authentication DTO: it has no fields for API
/// keys, client secrets, authorization codes, or tokens.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ProviderLoginSupplement {
    pub issuer: Option<String>,
    pub authorize_endpoint: Option<String>,
    pub device_authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub scopes: Option<Vec<String>>,
    pub client_id: Option<String>,
}

/// Parameters consumed by the existing generic OAuth transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OauthTransportParameters {
    Pkce {
        issuer: String,
        authorize_endpoint: String,
        token_endpoint: String,
        scopes: Vec<String>,
        client_id: String,
    },
    DeviceCode {
        issuer: String,
        device_authorization_endpoint: String,
        token_endpoint: String,
        scopes: Vec<String>,
        client_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ProviderLoginConversionError {
    #[error("provider login profile is missing client_id")]
    MissingClientId,
    #[error("provider login profile is missing issuer")]
    MissingIssuer,
    #[error("provider login profile is missing authorize endpoint")]
    MissingAuthorizeEndpoint,
    #[error("provider login profile is missing device authorization endpoint")]
    MissingDeviceAuthorizationEndpoint,
    #[error("provider login profile is missing token endpoint")]
    MissingTokenEndpoint,
    #[error("provider login profile is missing scopes")]
    MissingScopes,
    #[error("provider login method is not supported by the OAuth transport")]
    UnsupportedLoginMethod(ProviderLoginMethod),
}

const XAI_SCOPES: &[&str] = &[
    "openid",
    "profile",
    "email",
    "offline_access",
    "grok-cli:access",
    "api:access",
];
const GITHUB_COPILOT_SCOPES: &[&str] = &["read:user"];
const OPENAI_SCOPES: &[&str] = &["openid", "profile", "email", "offline_access"];

const OPENAI_ACCOUNT_TOKEN_ORDER: &[AccountTokenSource] =
    &[AccountTokenSource::IdToken, AccountTokenSource::AccessToken];
const OPENAI_ACCOUNT_CLAIMS: &[&str] = &[
    "chatgpt_account_id",
    "https://api.openai.com/auth.chatgpt_account_id",
    "organizations[0].id",
];

const XAI_PRIMARY_FIELDS: &[ProviderLoginField] = &[
    ProviderLoginField::ProviderId,
    ProviderLoginField::DisplayName,
    ProviderLoginField::LoginMethod,
    ProviderLoginField::DeviceAuthorizationEndpoint,
    ProviderLoginField::TokenEndpoint,
    ProviderLoginField::Scopes,
    ProviderLoginField::ClientId,
];
const XAI_ACCOUNT_FIELDS: &[ProviderLoginField] = &[ProviderLoginField::AccountIdentifier];
const XAI_EVIDENCE: &[ProviderLoginEvidence] = &[
    ProviderLoginEvidence {
        fields: XAI_PRIMARY_FIELDS,
        source: "../_ref/opencode/packages/opencode/src/plugin/xai.ts:5-17,208,297-324",
    },
    ProviderLoginEvidence {
        fields: XAI_ACCOUNT_FIELDS,
        source: "../_ref/opencode/packages/opencode/src/plugin/xai.ts:297-324",
    },
];

const GITHUB_PRIMARY_FIELDS: &[ProviderLoginField] = &[
    ProviderLoginField::ProviderId,
    ProviderLoginField::DisplayName,
    ProviderLoginField::LoginMethod,
    ProviderLoginField::DeviceAuthorizationEndpoint,
    ProviderLoginField::TokenEndpoint,
    ProviderLoginField::Scopes,
    ProviderLoginField::ClientId,
];
const GITHUB_ACCOUNT_FIELDS: &[ProviderLoginField] = &[ProviderLoginField::AccountIdentifier];
const GITHUB_EVIDENCE: &[ProviderLoginEvidence] = &[
    ProviderLoginEvidence {
        fields: GITHUB_PRIMARY_FIELDS,
        source: "../_ref/opencode/packages/opencode/src/plugin/github-copilot/copilot.ts:9-23,60-61,182-275",
    },
    ProviderLoginEvidence {
        fields: GITHUB_ACCOUNT_FIELDS,
        source: "../_ref/opencode/packages/opencode/src/plugin/github-copilot/copilot.ts:280-305",
    },
];

const ANTHROPIC_FIELDS: &[ProviderLoginField] = &[
    ProviderLoginField::ProviderId,
    ProviderLoginField::DisplayName,
    ProviderLoginField::LoginMethod,
];
const ANTHROPIC_EVIDENCE: &[ProviderLoginEvidence] = &[ProviderLoginEvidence {
    fields: ANTHROPIC_FIELDS,
    source: "../_ref/opencode/packages/llm/src/providers/anthropic.ts:7-17",
}];

const OPENAI_PRIMARY_FIELDS: &[ProviderLoginField] = &[
    ProviderLoginField::ProviderId,
    ProviderLoginField::DisplayName,
    ProviderLoginField::LoginMethod,
    ProviderLoginField::Issuer,
    ProviderLoginField::AuthorizeEndpoint,
    ProviderLoginField::TokenEndpoint,
    ProviderLoginField::Scopes,
    ProviderLoginField::ClientId,
];
const OPENAI_ACCOUNT_FIELDS: &[ProviderLoginField] = &[ProviderLoginField::AccountIdentifier];
const OPENAI_EVIDENCE: &[ProviderLoginEvidence] = &[
    ProviderLoginEvidence {
        fields: OPENAI_PRIMARY_FIELDS,
        source: "../_ref/opencode/packages/opencode/src/plugin/openai/codex.ts:10-11,78-92,107-133,320-321,429-454",
    },
    ProviderLoginEvidence {
        fields: OPENAI_ACCOUNT_FIELDS,
        source: "../_ref/opencode/packages/opencode/src/plugin/openai/codex.ts:38-75",
    },
];

const CUSTOM_FIELDS: &[ProviderLoginField] = &[
    ProviderLoginField::ProviderId,
    ProviderLoginField::DisplayName,
    ProviderLoginField::LoginMethod,
    ProviderLoginField::Issuer,
    ProviderLoginField::AuthorizeEndpoint,
    ProviderLoginField::TokenEndpoint,
    ProviderLoginField::Scopes,
    ProviderLoginField::ClientId,
    ProviderLoginField::AccountIdentifier,
];
const CUSTOM_EVIDENCE: &[ProviderLoginEvidence] = &[
    ProviderLoginEvidence {
        fields: CUSTOM_FIELDS,
        source: "https://openid.net/specs/openid-connect-discovery-1_0.html",
    },
    ProviderLoginEvidence {
        fields: CUSTOM_FIELDS,
        source: "https://www.rfc-editor.org/rfc/rfc7636",
    },
];

const PROVIDER_LOGIN_PROFILES: &[ProviderLoginProfile] = &[
    ProviderLoginProfile {
        provider_id: "xai",
        display_name: "xAI",
        login_method: ProviderLoginMethod::DeviceCode,
        issuer: ProfileField::Unknown,
        authorize_endpoint: ProfileField::NotApplicable,
        device_authorization_endpoint: ProfileField::Known("https://auth.x.ai/oauth2/device/code"),
        token_endpoint: ProfileField::Known("https://auth.x.ai/oauth2/token"),
        scopes: ProfileField::Known(XAI_SCOPES),
        client_id: ClientIdProfile {
            source: ClientIdSource::Unknown,
            value: ProfileField::Unknown,
        },
        account_identifier: AccountIdentifierStrategy::Unknown,
        evidence: XAI_EVIDENCE,
    },
    ProviderLoginProfile {
        provider_id: "github-copilot",
        display_name: "GitHub Copilot",
        login_method: ProviderLoginMethod::DeviceCode,
        issuer: ProfileField::Unknown,
        authorize_endpoint: ProfileField::NotApplicable,
        device_authorization_endpoint: ProfileField::Known("https://github.com/login/device/code"),
        token_endpoint: ProfileField::Known("https://github.com/login/oauth/access_token"),
        scopes: ProfileField::Known(GITHUB_COPILOT_SCOPES),
        client_id: ClientIdProfile {
            source: ClientIdSource::Unknown,
            value: ProfileField::Unknown,
        },
        account_identifier: AccountIdentifierStrategy::Unknown,
        evidence: GITHUB_EVIDENCE,
    },
    ProviderLoginProfile {
        provider_id: "anthropic",
        display_name: "Anthropic",
        login_method: ProviderLoginMethod::ApiKeyOnly,
        issuer: ProfileField::NotApplicable,
        authorize_endpoint: ProfileField::NotApplicable,
        device_authorization_endpoint: ProfileField::NotApplicable,
        token_endpoint: ProfileField::NotApplicable,
        scopes: ProfileField::NotApplicable,
        client_id: ClientIdProfile {
            source: ClientIdSource::Unknown,
            value: ProfileField::NotApplicable,
        },
        account_identifier: AccountIdentifierStrategy::Unknown,
        evidence: ANTHROPIC_EVIDENCE,
    },
    ProviderLoginProfile {
        provider_id: "openai",
        display_name: "OpenAI",
        login_method: ProviderLoginMethod::Pkce,
        issuer: ProfileField::Known("https://auth.openai.com"),
        authorize_endpoint: ProfileField::Known("https://auth.openai.com/oauth/authorize"),
        device_authorization_endpoint: ProfileField::NotApplicable,
        token_endpoint: ProfileField::Known("https://auth.openai.com/oauth/token"),
        scopes: ProfileField::Known(OPENAI_SCOPES),
        client_id: ClientIdProfile {
            source: ClientIdSource::Unknown,
            value: ProfileField::Unknown,
        },
        account_identifier: AccountIdentifierStrategy::TokenClaims {
            token_order: OPENAI_ACCOUNT_TOKEN_ORDER,
            claim_paths: OPENAI_ACCOUNT_CLAIMS,
        },
        evidence: OPENAI_EVIDENCE,
    },
    ProviderLoginProfile {
        provider_id: "custom-oidc",
        display_name: "Custom OIDC",
        login_method: ProviderLoginMethod::Pkce,
        issuer: ProfileField::UserSupplied,
        authorize_endpoint: ProfileField::UserSupplied,
        device_authorization_endpoint: ProfileField::NotApplicable,
        token_endpoint: ProfileField::UserSupplied,
        scopes: ProfileField::UserSupplied,
        client_id: ClientIdProfile {
            source: ClientIdSource::UserSupplied,
            value: ProfileField::UserSupplied,
        },
        account_identifier: AccountIdentifierStrategy::UserSupplied,
        evidence: CUSTOM_EVIDENCE,
    },
];

pub fn provider_login_profiles() -> &'static [ProviderLoginProfile] {
    PROVIDER_LOGIN_PROFILES
}

pub fn provider_login_profile(provider_id: &str) -> Option<&'static ProviderLoginProfile> {
    PROVIDER_LOGIN_PROFILES
        .iter()
        .find(|profile| profile.provider_id == provider_id)
}

/// Resolve a provider profile into the exact public parameters needed by the
/// generic OAuth transport. Known catalog values cannot be overridden by the
/// supplement; only `UserSupplied` and `Unknown` fields are fillable.
pub fn resolve_oauth_transport_parameters(
    profile: &ProviderLoginProfile,
    supplement: &ProviderLoginSupplement,
) -> Result<OauthTransportParameters, ProviderLoginConversionError> {
    if profile.login_method == ProviderLoginMethod::ApiKeyOnly {
        return Err(ProviderLoginConversionError::UnsupportedLoginMethod(
            profile.login_method,
        ));
    }

    let issuer = resolve_string(profile.issuer, supplement.issuer.as_deref())
        .ok_or(ProviderLoginConversionError::MissingIssuer)?;
    let client_id = match profile.client_id.source {
        ClientIdSource::PublicDocumented => resolve_string(profile.client_id.value, None),
        ClientIdSource::UserSupplied | ClientIdSource::Unknown => supplement
            .client_id
            .as_deref()
            .and_then(normalized_non_empty),
    }
    .ok_or(ProviderLoginConversionError::MissingClientId)?;
    let token_endpoint =
        resolve_string(profile.token_endpoint, supplement.token_endpoint.as_deref())
            .ok_or(ProviderLoginConversionError::MissingTokenEndpoint)?;
    let scopes = resolve_scopes(profile.scopes, supplement.scopes.as_deref())
        .ok_or(ProviderLoginConversionError::MissingScopes)?;

    match profile.login_method {
        ProviderLoginMethod::Pkce => {
            let authorize_endpoint = resolve_string(
                profile.authorize_endpoint,
                supplement.authorize_endpoint.as_deref(),
            )
            .ok_or(ProviderLoginConversionError::MissingAuthorizeEndpoint)?;
            Ok(OauthTransportParameters::Pkce {
                issuer,
                authorize_endpoint,
                token_endpoint,
                scopes,
                client_id,
            })
        }
        ProviderLoginMethod::DeviceCode => {
            let device_authorization_endpoint = resolve_string(
                profile.device_authorization_endpoint,
                supplement.device_authorization_endpoint.as_deref(),
            )
            .ok_or(ProviderLoginConversionError::MissingDeviceAuthorizationEndpoint)?;
            Ok(OauthTransportParameters::DeviceCode {
                issuer,
                device_authorization_endpoint,
                token_endpoint,
                scopes,
                client_id,
            })
        }
        ProviderLoginMethod::ApiKeyOnly => unreachable!("handled before public field resolution"),
    }
}

fn resolve_string(field: ProfileField<&str>, supplement: Option<&str>) -> Option<String> {
    match field {
        ProfileField::Known(value) => normalized_non_empty(value),
        ProfileField::UserSupplied | ProfileField::Unknown => {
            supplement.and_then(normalized_non_empty)
        }
        ProfileField::NotApplicable => None,
    }
}

fn resolve_scopes(
    field: ProfileField<&[&str]>,
    supplement: Option<&[String]>,
) -> Option<Vec<String>> {
    match field {
        ProfileField::Known(values) => normalize_scopes(values.iter().copied()),
        ProfileField::UserSupplied | ProfileField::Unknown => {
            normalize_scopes(supplement?.iter().map(String::as_str))
        }
        ProfileField::NotApplicable => None,
    }
}

fn normalized_non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && !value.chars().any(char::is_control)).then(|| value.to_owned())
}

fn normalize_scopes<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<Vec<String>> {
    let values = values
        .into_iter()
        .map(normalized_non_empty)
        .collect::<Option<Vec<_>>>()?;
    (!values.is_empty()).then_some(values)
}
