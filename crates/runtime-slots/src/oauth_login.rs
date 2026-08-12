//! End-to-end OAuth login orchestration for slot create.
//!
//! Composes PKCE loopback / device-code transport with a progress callback so
//! the host can surface a copyable authorize URL when the system browser fails
//! (OpenCode pattern: always expose the URL, still attempt open, still wait).
//!
//! Never logs tokens, codes, or client secrets.

use std::time::{Duration, SystemTime};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};

use crate::oauth::{TokenResponse, build_authorize_url, expiry_from_expires_in, generate_pkce};
use crate::oauth_transport::{
    AuthorizeUrlPolicy, BrowserOpener, DEFAULT_LOOPBACK_TIMEOUT, LoopbackListener, PollClock,
    SystemClock, TokenEndpointPolicy, exchange_authorization_code, open_authorize_with,
    poll_device_token, request_device_authorization, validate_token_endpoint,
};
use crate::provider_login::{
    OauthTransportParameters, ProviderLoginConversionError, ProviderLoginProfile,
    ProviderLoginSupplement, resolve_oauth_transport_parameters,
};
use crate::secret::SecretBytes;
use crate::{OauthTokenClient, RuntimeSlotError};

/// Default loopback wait used by production login (tests inject shorter).
pub const DEFAULT_OAUTH_LOGIN_LOOPBACK_TIMEOUT: Duration = DEFAULT_LOOPBACK_TIMEOUT;

/// Public progress for UI (no secrets). Host may emit this as a Tauri event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OauthLoginProgressEvent {
    /// Authorize or device verification URL the user can open manually.
    pub authorize_url: String,
    /// `true` when the system browser open attempt returned Ok.
    pub browser_opened: bool,
    /// When device-code: user-facing code (never the secret device_code).
    pub user_code: Option<String>,
}

/// Injectable progress sink (host emits UI events; tests record).
pub trait OauthLoginProgress: Send + Sync {
    fn on_authorize_ready(&self, event: &OauthLoginProgressEvent);
}

/// No-op progress (tests that do not care about UI surface).
pub struct NullOauthLoginProgress;

impl OauthLoginProgress for NullOauthLoginProgress {
    fn on_authorize_ready(&self, _event: &OauthLoginProgressEvent) {}
}

/// Recording progress for unit tests.
#[derive(Default)]
pub struct RecordingOauthLoginProgress {
    pub events: std::sync::Mutex<Vec<OauthLoginProgressEvent>>,
}

impl OauthLoginProgress for RecordingOauthLoginProgress {
    fn on_authorize_ready(&self, event: &OauthLoginProgressEvent) {
        if let Ok(mut guard) = self.events.lock() {
            guard.push(event.clone());
        }
    }
}

/// Inputs for a single login attempt (no tokens).
#[derive(Debug, Clone)]
pub struct OauthLoginInput {
    pub provider_id: String,
    pub base_url: String,
    pub model_id: String,
    pub supplement: ProviderLoginSupplement,
    /// Optional non-secret account label (user-supplied strategy).
    pub account_label: Option<String>,
}

/// Successful login material ready for `CreateSlotAuth::Oauth`.
pub struct OauthLoginTokens {
    pub provider_id: String,
    pub issuer: String,
    pub client_id: String,
    pub scopes: Vec<String>,
    pub token_endpoint: String,
    pub account_label: Option<String>,
    pub base_url: String,
    pub model_id: String,
    pub access_token: SecretBytes,
    pub refresh_token: SecretBytes,
    pub access_expires_at: DateTime<Utc>,
}

impl std::fmt::Debug for OauthLoginTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OauthLoginTokens")
            .field("provider_id", &self.provider_id)
            .field("issuer", &self.issuer)
            .field("client_id", &self.client_id)
            .field("scopes", &self.scopes)
            .field("token_endpoint", &self.token_endpoint)
            .field("account_label", &self.account_label)
            .field("base_url", &self.base_url)
            .field("model_id", &self.model_id)
            .field("access_token", &"SecretBytes([REDACTED])")
            .field("refresh_token", &"SecretBytes([REDACTED])")
            .field("access_expires_at", &self.access_expires_at)
            .finish()
    }
}

/// Login failure with optional public authorize URL for manual open fallback.
#[derive(Debug)]
pub struct OauthLoginFailure {
    pub error: RuntimeSlotError,
    /// Public authorize / verification URL when available (never a token).
    pub authorize_url: Option<String>,
}

impl OauthLoginFailure {
    pub fn code(&self) -> &'static str {
        self.error.code()
    }

    fn from_error(error: RuntimeSlotError) -> Self {
        Self {
            error,
            authorize_url: None,
        }
    }

    fn with_url(error: RuntimeSlotError, authorize_url: String) -> Self {
        Self {
            error,
            authorize_url: Some(authorize_url),
        }
    }
}

impl From<ProviderLoginConversionError> for OauthLoginFailure {
    fn from(value: ProviderLoginConversionError) -> Self {
        // Incomplete catalog supplement → invalid input (no secrets).
        let _ = value;
        Self::from_error(RuntimeSlotError::InvalidInput)
    }
}

/// Tunables for production vs tests.
#[derive(Debug, Clone)]
pub struct OauthLoginOptions {
    pub loopback_timeout: Duration,
    pub authorize_policy: AuthorizeUrlPolicy,
    pub token_policy: TokenEndpointPolicy,
}

impl Default for OauthLoginOptions {
    fn default() -> Self {
        Self {
            loopback_timeout: DEFAULT_OAUTH_LOGIN_LOOPBACK_TIMEOUT,
            authorize_policy: AuthorizeUrlPolicy::default(),
            token_policy: TokenEndpointPolicy::default(),
        }
    }
}

/// Run provider OAuth login and return tokens (does not persist a slot).
///
/// Browser open failures are **non-terminal**: progress is reported with
/// `browser_opened: false` and the authorize URL so the UI can show a
/// copyable fallback, then the flow still waits for the callback (OpenCode
/// pattern). Terminal browser-unavailable is only returned when the opener
/// fails **and** the caller sets `fail_closed_on_browser` (test-only).
// Arity is the flow itself: profile, caller input, browser, clock,
// transport and callbacks are independent collaborators. Bundling them
// into a struct would only move the same eight names one level down.
#[allow(clippy::too_many_arguments)]
pub fn perform_oauth_login(
    profile: &ProviderLoginProfile,
    input: &OauthLoginInput,
    browser: &dyn BrowserOpener,
    token_client: &dyn OauthTokenClient,
    progress: &dyn OauthLoginProgress,
    options: &OauthLoginOptions,
    clock: &dyn PollClock,
    fail_closed_on_browser: bool,
) -> Result<OauthLoginTokens, OauthLoginFailure> {
    if profile.provider_id != input.provider_id {
        return Err(OauthLoginFailure::from_error(
            RuntimeSlotError::InvalidInput,
        ));
    }
    let params = resolve_oauth_transport_parameters(profile, &input.supplement)?;
    match params {
        OauthTransportParameters::Pkce {
            issuer,
            authorize_endpoint,
            token_endpoint,
            scopes,
            client_id,
        } => perform_pkce_login(
            input,
            browser,
            token_client,
            progress,
            options,
            fail_closed_on_browser,
            issuer,
            authorize_endpoint,
            token_endpoint,
            scopes,
            client_id,
        ),
        OauthTransportParameters::DeviceCode {
            issuer,
            device_authorization_endpoint,
            token_endpoint,
            scopes,
            client_id,
        } => perform_device_login(
            input,
            browser,
            token_client,
            progress,
            options,
            clock,
            fail_closed_on_browser,
            issuer,
            device_authorization_endpoint,
            token_endpoint,
            scopes,
            client_id,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn perform_pkce_login(
    input: &OauthLoginInput,
    browser: &dyn BrowserOpener,
    token_client: &dyn OauthTokenClient,
    progress: &dyn OauthLoginProgress,
    options: &OauthLoginOptions,
    fail_closed_on_browser: bool,
    issuer: String,
    authorize_endpoint: String,
    token_endpoint: String,
    scopes: Vec<String>,
    client_id: String,
) -> Result<OauthLoginTokens, OauthLoginFailure> {
    validate_token_endpoint(&token_endpoint, options.token_policy)
        .map_err(OauthLoginFailure::from_error)?;

    let listener = LoopbackListener::bind().map_err(OauthLoginFailure::from_error)?;
    let redirect_uri = listener.redirect_uri().to_owned();
    let pkce = generate_pkce().map_err(OauthLoginFailure::from_error)?;
    let state = random_oauth_state().map_err(OauthLoginFailure::from_error)?;
    let authorize_url = build_authorize_url(
        &authorize_endpoint,
        &client_id,
        &redirect_uri,
        &scopes,
        &pkce.challenge,
        &state,
    )
    .map_err(OauthLoginFailure::from_error)?;

    let browser_opened =
        match open_authorize_with(browser, &authorize_url, &issuer, options.authorize_policy) {
            Ok(()) => true,
            Err(RuntimeSlotError::OauthBrowserOpenFailed) => {
                if fail_closed_on_browser {
                    progress.on_authorize_ready(&OauthLoginProgressEvent {
                        authorize_url: authorize_url.clone(),
                        browser_opened: false,
                        user_code: None,
                    });
                    return Err(OauthLoginFailure::with_url(
                        RuntimeSlotError::OauthBrowserOpenFailed,
                        authorize_url,
                    ));
                }
                false
            }
            Err(other) => {
                // Invalid authorize URL etc. — fail closed with no wait.
                return Err(OauthLoginFailure::from_error(other));
            }
        };

    progress.on_authorize_ready(&OauthLoginProgressEvent {
        authorize_url: authorize_url.clone(),
        browser_opened,
        user_code: None,
    });

    let code = listener
        .wait_for_code(&state, options.loopback_timeout)
        .map_err(|error| OauthLoginFailure::with_url(error, authorize_url.clone()))?;

    let tokens = exchange_authorization_code(
        token_client,
        &token_endpoint,
        &client_id,
        code.as_secret(),
        &redirect_uri,
        &pkce.verifier,
    )
    .map_err(|error| OauthLoginFailure::with_url(error, authorize_url))?;

    finish_tokens(input, issuer, client_id, scopes, token_endpoint, tokens)
}

#[allow(clippy::too_many_arguments)]
fn perform_device_login(
    input: &OauthLoginInput,
    browser: &dyn BrowserOpener,
    token_client: &dyn OauthTokenClient,
    progress: &dyn OauthLoginProgress,
    options: &OauthLoginOptions,
    clock: &dyn PollClock,
    fail_closed_on_browser: bool,
    issuer: String,
    device_authorization_endpoint: String,
    token_endpoint: String,
    scopes: Vec<String>,
    client_id: String,
) -> Result<OauthLoginTokens, OauthLoginFailure> {
    validate_token_endpoint(&token_endpoint, options.token_policy)
        .map_err(OauthLoginFailure::from_error)?;
    validate_token_endpoint(&device_authorization_endpoint, options.token_policy)
        .map_err(OauthLoginFailure::from_error)?;

    let device = request_device_authorization(
        token_client,
        &device_authorization_endpoint,
        &client_id,
        &scopes,
    )
    .map_err(OauthLoginFailure::from_error)?;

    let authorize_url = device
        .verification_uri_complete
        .clone()
        .unwrap_or_else(|| device.verification_uri.clone());

    // Device verification URIs are https (or test loopback when policy allows).
    // Skip issuer-prefix authorize policy; open via browser opener directly after
    // a basic non-empty check (opener rejects control chars / empty).
    let browser_opened = match browser.open(&authorize_url) {
        Ok(()) => true,
        Err(RuntimeSlotError::OauthBrowserOpenFailed) => {
            if fail_closed_on_browser {
                progress.on_authorize_ready(&OauthLoginProgressEvent {
                    authorize_url: authorize_url.clone(),
                    browser_opened: false,
                    user_code: Some(device.user_code.clone()),
                });
                return Err(OauthLoginFailure::with_url(
                    RuntimeSlotError::OauthBrowserOpenFailed,
                    authorize_url,
                ));
            }
            false
        }
        Err(other) => return Err(OauthLoginFailure::from_error(other)),
    };

    progress.on_authorize_ready(&OauthLoginProgressEvent {
        authorize_url: authorize_url.clone(),
        browser_opened,
        user_code: Some(device.user_code.clone()),
    });

    let tokens = poll_device_token(token_client, &token_endpoint, &client_id, &device, clock)
        .map_err(|error| OauthLoginFailure::with_url(error, authorize_url))?;

    finish_tokens(input, issuer, client_id, scopes, token_endpoint, tokens)
}

fn finish_tokens(
    input: &OauthLoginInput,
    issuer: String,
    client_id: String,
    scopes: Vec<String>,
    token_endpoint: String,
    tokens: TokenResponse,
) -> Result<OauthLoginTokens, OauthLoginFailure> {
    let refresh = tokens
        .refresh_token
        .ok_or_else(|| OauthLoginFailure::from_error(RuntimeSlotError::OauthTokenMissingField))?;
    if tokens.access_token.is_empty() || refresh.is_empty() {
        return Err(OauthLoginFailure::from_error(
            RuntimeSlotError::OauthTokenMissingField,
        ));
    }
    let expires_in = tokens.expires_in.unwrap_or(3600);
    let expires_at_sys = expiry_from_expires_in(SystemTime::now(), expires_in);
    let access_expires_at = system_time_to_utc(expires_at_sys);

    Ok(OauthLoginTokens {
        provider_id: input.provider_id.clone(),
        issuer,
        client_id,
        scopes,
        token_endpoint,
        account_label: input
            .account_label
            .as_ref()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty()),
        base_url: input.base_url.clone(),
        model_id: input.model_id.clone(),
        access_token: tokens.access_token,
        refresh_token: refresh,
        access_expires_at,
    })
}

fn random_oauth_state() -> Result<String, RuntimeSlotError> {
    let mut raw = [0u8; 32];
    getrandom::getrandom(&mut raw).map_err(|_| RuntimeSlotError::InvalidInput)?;
    Ok(URL_SAFE_NO_PAD.encode(raw))
}

fn system_time_to_utc(value: SystemTime) -> DateTime<Utc> {
    match value.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => {
            DateTime::<Utc>::from_timestamp(duration.as_secs() as i64, duration.subsec_nanos())
                .unwrap_or_else(Utc::now)
        }
        Err(_) => Utc::now(),
    }
}

/// Convenience for production host: wall clock + default options.
pub fn perform_oauth_login_production(
    profile: &ProviderLoginProfile,
    input: &OauthLoginInput,
    browser: &dyn BrowserOpener,
    token_client: &dyn OauthTokenClient,
    progress: &dyn OauthLoginProgress,
) -> Result<OauthLoginTokens, OauthLoginFailure> {
    perform_oauth_login(
        profile,
        input,
        browser,
        token_client,
        progress,
        &OauthLoginOptions::default(),
        &SystemClock,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RuntimeSlotError;
    use crate::oauth_transport::{RecordingBrowser, VirtualClock};
    use crate::provider_login::{ProviderLoginSupplement, provider_login_profile};
    use std::sync::{Arc, Mutex};

    struct SeqTokenClient {
        bodies: Mutex<Vec<Result<String, RuntimeSlotError>>>,
    }

    impl SeqTokenClient {
        fn new(bodies: Vec<Result<String, RuntimeSlotError>>) -> Self {
            Self {
                bodies: Mutex::new(bodies),
            }
        }
    }

    impl OauthTokenClient for SeqTokenClient {
        fn post_token_form(
            &self,
            _token_endpoint: &str,
            _form_body: &str,
        ) -> Result<String, RuntimeSlotError> {
            let mut guard = self.bodies.lock().unwrap();
            if guard.is_empty() {
                return Err(RuntimeSlotError::OauthHttpUnavailable);
            }
            guard.remove(0)
        }
    }

    #[test]
    fn browser_open_fail_closed_returns_copyable_authorize_url() {
        let profile = provider_login_profile("custom-oidc").unwrap();
        let browser = RecordingBrowser {
            fail: true,
            ..Default::default()
        };
        let progress = RecordingOauthLoginProgress::default();
        let client = SeqTokenClient::new(vec![]);
        let input = OauthLoginInput {
            provider_id: "custom-oidc".into(),
            base_url: "https://api.example.test/v1".into(),
            model_id: "model-1".into(),
            supplement: ProviderLoginSupplement {
                issuer: Some("https://as.example.test".into()),
                authorize_endpoint: Some("https://as.example.test/oauth/authorize".into()),
                token_endpoint: Some("https://as.example.test/oauth/token".into()),
                scopes: Some(vec!["openid".into()]),
                client_id: Some("client-abc".into()),
                ..Default::default()
            },
            account_label: None,
        };
        let options = OauthLoginOptions {
            loopback_timeout: Duration::from_millis(50),
            authorize_policy: AuthorizeUrlPolicy {
                allow_loopback_http: false,
            },
            token_policy: TokenEndpointPolicy {
                allow_loopback_http: false,
            },
        };
        let err = perform_oauth_login(
            profile,
            &input,
            &browser,
            &client,
            &progress,
            &options,
            &VirtualClock::new(),
            true, // fail closed so we do not block on loopback in this unit test
        )
        .expect_err("browser fail must error when fail_closed_on_browser");
        assert_eq!(err.code(), "runtime_slot_oauth_browser_open_failed");
        let url = err
            .authorize_url
            .as_deref()
            .expect("authorize url fallback");
        assert!(url.starts_with("https://as.example.test/oauth/authorize"));
        assert!(url.contains("client_id=client-abc"));
        assert!(url.contains("code_challenge"));
        // Progress fired before terminal return.
        let events = progress.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(!events[0].browser_opened);
        assert_eq!(events[0].authorize_url, url);
        // No secret-looking token material in the public URL/error path.
        assert!(!url.contains("access_token"));
        assert!(!format!("{err:?}").contains("sk-"));
    }

    #[test]
    fn browser_open_fail_still_reports_progress_then_times_out_with_url() {
        let profile = provider_login_profile("custom-oidc").unwrap();
        let browser = RecordingBrowser {
            fail: true,
            ..Default::default()
        };
        let progress = RecordingOauthLoginProgress::default();
        let client = SeqTokenClient::new(vec![]);
        let input = OauthLoginInput {
            provider_id: "custom-oidc".into(),
            base_url: "https://api.example.test/v1".into(),
            model_id: "model-1".into(),
            supplement: ProviderLoginSupplement {
                issuer: Some("https://as.example.test".into()),
                authorize_endpoint: Some("https://as.example.test/oauth/authorize".into()),
                token_endpoint: Some("https://as.example.test/oauth/token".into()),
                scopes: Some(vec!["openid".into()]),
                client_id: Some("client-abc".into()),
                ..Default::default()
            },
            account_label: None,
        };
        let options = OauthLoginOptions {
            loopback_timeout: Duration::from_millis(80),
            authorize_policy: AuthorizeUrlPolicy::default(),
            token_policy: TokenEndpointPolicy::default(),
        };
        let err = perform_oauth_login(
            profile,
            &input,
            &browser,
            &client,
            &progress,
            &options,
            &VirtualClock::new(),
            false, // production: keep waiting after browser fail
        )
        .expect_err("loopback must time out");
        assert_eq!(err.code(), "runtime_slot_oauth_loopback_timeout");
        let url = err.authorize_url.expect("url kept on timeout");
        assert!(url.contains("https://as.example.test/oauth/authorize"));
        let events = progress.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(!events[0].browser_opened);
        assert_eq!(events[0].authorize_url, url);
    }

    #[test]
    fn device_browser_fail_closed_returns_verification_uri() {
        let profile = provider_login_profile("xai").unwrap();
        let browser = RecordingBrowser {
            fail: true,
            ..Default::default()
        };
        let progress = RecordingOauthLoginProgress::default();
        // Device authorization response (public fields only in body; device_code is secret).
        let device_body = r#"{
            "device_code":"dev-secret-code-not-for-ui",
            "user_code":"ABCD-1234",
            "verification_uri":"https://auth.x.ai/device",
            "expires_in":600,
            "interval":5
        }"#;
        let client = SeqTokenClient::new(vec![Ok(device_body.into())]);
        let input = OauthLoginInput {
            provider_id: "xai".into(),
            base_url: "https://api.x.ai/v1".into(),
            model_id: "grok-3".into(),
            supplement: ProviderLoginSupplement {
                // xAI catalog marks issuer + client_id Unknown — must supply both.
                issuer: Some("https://auth.x.ai".into()),
                client_id: Some("xai-client".into()),
                ..Default::default()
            },
            account_label: None,
        };
        let err = perform_oauth_login(
            profile,
            &input,
            &browser,
            &client,
            &progress,
            &OauthLoginOptions::default(),
            &VirtualClock::new(),
            true,
        )
        .expect_err("browser fail closed");
        assert_eq!(err.code(), "runtime_slot_oauth_browser_open_failed");
        assert_eq!(
            err.authorize_url.as_deref(),
            Some("https://auth.x.ai/device")
        );
        let events = progress.events.lock().unwrap();
        assert_eq!(events[0].user_code.as_deref(), Some("ABCD-1234"));
        // Secret device_code must not appear on the public failure surface.
        let dbg = format!("{err:?}");
        assert!(!dbg.contains("dev-secret-code-not-for-ui"));
    }

    #[test]
    fn missing_client_id_is_invalid_input_without_url() {
        let profile = provider_login_profile("openai").unwrap();
        let err = perform_oauth_login(
            profile,
            &OauthLoginInput {
                provider_id: "openai".into(),
                base_url: "https://api.openai.com/v1".into(),
                model_id: "gpt-4o".into(),
                supplement: ProviderLoginSupplement::default(),
                account_label: None,
            },
            &RecordingBrowser::default(),
            &SeqTokenClient::new(vec![]),
            &NullOauthLoginProgress,
            &OauthLoginOptions::default(),
            &VirtualClock::new(),
            true,
        )
        .expect_err("missing client id");
        assert_eq!(err.code(), "runtime_slot_input_invalid");
        assert!(err.authorize_url.is_none());
    }

    #[test]
    fn progress_event_is_send_sync_for_host_emit() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<OauthLoginProgressEvent>();
        assert_send_sync::<Arc<RecordingOauthLoginProgress>>();
    }
}
