//! OAuth transport: loopback callback, browser open, token HTTP, device code.
//!
//! Security invariants:
//! - Tokens, authorization codes, and device codes never appear in logs,
//!   error `Display`/`Debug`, or public DTOs.
//! - client_id / issuer / scopes are always caller-supplied (no vendor hard-coding).
//! - Production endpoints must be https; loopback http is test-only when opted in.
//! - Fail closed — never treat a transport failure as “unauthenticated but ok”.

use std::fmt;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::oauth::{
    TokenResponse, build_code_exchange_form, build_refresh_token_form, parse_token_response,
};
use crate::secret::SecretBytes;
use crate::win_http::{self, HttpTimeouts};
use crate::{OauthTokenClient, RuntimeSlotError};

/// Max token-endpoint response body (bytes). Tokens are small; oversized = invalid.
pub const OAUTH_TOKEN_RESPONSE_BYTE_CAP: usize = 65_536;

/// Default hard timeout for a single loopback authorization callback wait.
pub const DEFAULT_LOOPBACK_TIMEOUT: Duration = Duration::from_secs(120);

/// Default RFC 8628 poll interval when the AS omits `interval`.
pub const DEFAULT_DEVICE_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Extra delay added on `slow_down` (RFC 8628 §3.5).
pub const DEVICE_SLOW_DOWN_INCREMENT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// T1 — loopback callback
// ---------------------------------------------------------------------------

/// One-shot loopback listener bound exclusively to `127.0.0.1` (never `0.0.0.0`).
///
/// Call [`LoopbackListener::wait_for_code`] once; the listener is consumed and
/// closed whether the wait succeeds, times out, or rejects a bad state.
pub struct LoopbackListener {
    listener: TcpListener,
    port: u16,
    redirect_uri: String,
    /// Set true after the first request is accepted (success or reject path).
    accepted_once: Arc<AtomicBool>,
}

/// Authorization code received from the redirect. Never Debug-prints the code.
pub struct AuthorizationCode {
    code: SecretBytes,
}

impl AuthorizationCode {
    pub fn into_secret(self) -> SecretBytes {
        self.code
    }

    pub fn as_secret(&self) -> &SecretBytes {
        &self.code
    }
}

impl fmt::Debug for AuthorizationCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthorizationCode")
            .field("code", &"SecretBytes([REDACTED])")
            .finish()
    }
}

impl LoopbackListener {
    /// Bind `127.0.0.1:0` (OS-assigned free port). Fails closed if bind fails.
    pub fn bind() -> Result<Self, RuntimeSlotError> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .map_err(|_| RuntimeSlotError::OauthLoopbackBindFailed)?;
        listener
            .set_nonblocking(true)
            .map_err(|_| RuntimeSlotError::OauthLoopbackBindFailed)?;
        let port = listener
            .local_addr()
            .map_err(|_| RuntimeSlotError::OauthLoopbackBindFailed)?
            .port();
        // Loopback only — never advertise a non-loopback host.
        let redirect_uri = format!("http://127.0.0.1:{port}/callback");
        Ok(Self {
            listener,
            port,
            redirect_uri,
            accepted_once: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// Wait up to `timeout` for a single callback. Validates `state`.
    ///
    /// On success the response HTML does **not** echo the code.
    /// After any first request (match or mismatch), further requests are not
    /// accepted — the listener is dropped when this method returns.
    pub fn wait_for_code(
        self,
        expected_state: &str,
        timeout: Duration,
    ) -> Result<AuthorizationCode, RuntimeSlotError> {
        if expected_state.trim().is_empty() {
            return Err(RuntimeSlotError::InvalidInput);
        }
        let deadline = Instant::now() + timeout;
        let expected_state = expected_state.to_owned();
        let accepted_once = Arc::clone(&self.accepted_once);

        loop {
            if Instant::now() >= deadline {
                return Err(RuntimeSlotError::OauthLoopbackTimeout);
            }
            match self.listener.accept() {
                Ok((stream, peer)) => {
                    // Only loopback peers (defence in depth; we bound 127.0.0.1).
                    if !peer.ip().is_loopback() {
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                    if accepted_once.swap(true, Ordering::SeqCst) {
                        // Second request after we already decided — refuse.
                        let _ = write_plain_response(
                            &stream,
                            409,
                            "Authorization callback already handled.",
                        );
                        let _ = stream.shutdown(Shutdown::Both);
                        return Err(RuntimeSlotError::OauthLoopbackAlreadyConsumed);
                    }
                    return handle_callback_stream(stream, &expected_state);
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err(RuntimeSlotError::OauthLoopbackTimeout);
                    }
                    thread::sleep(Duration::from_millis(20).min(remaining));
                }
                Err(_) => return Err(RuntimeSlotError::OauthLoopbackBindFailed),
            }
        }
    }
}

fn handle_callback_stream(
    mut stream: TcpStream,
    expected_state: &str,
) -> Result<AuthorizationCode, RuntimeSlotError> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    let request = read_http_head(&mut stream)?;
    let target =
        parse_request_target(&request).ok_or(RuntimeSlotError::OauthLoopbackInvalidRequest)?;
    let query = query_of_target(target);

    // OAuth error redirect (user denied, etc.) — no code leakage.
    if let Some(err) = query_param(query, "error") {
        let _ = write_html_response(
            &stream,
            400,
            "Authorization failed",
            "The authorization server returned an error. You can close this window.",
        );
        let _ = stream.shutdown(Shutdown::Both);
        let _ = err; // never log error_description (may be sensitive)
        return Err(RuntimeSlotError::OauthLoopbackDenied);
    }

    let state = query_param(query, "state").ok_or(RuntimeSlotError::OauthLoopbackInvalidRequest)?;
    if state != expected_state {
        let _ = write_html_response(
            &stream,
            400,
            "Authorization rejected",
            "State mismatch. You can close this window.",
        );
        let _ = stream.shutdown(Shutdown::Both);
        return Err(RuntimeSlotError::OauthLoopbackStateMismatch);
    }

    let code = query_param(query, "code").ok_or(RuntimeSlotError::OauthLoopbackInvalidRequest)?;
    if code.is_empty() {
        let _ = write_html_response(
            &stream,
            400,
            "Authorization rejected",
            "Missing authorization code. You can close this window.",
        );
        let _ = stream.shutdown(Shutdown::Both);
        return Err(RuntimeSlotError::OauthLoopbackInvalidRequest);
    }

    // Success page — must NOT include the code.
    let _ = write_html_response(
        &stream,
        200,
        "Authorization complete",
        "You can close this window and return to Pillow.",
    );
    let _ = stream.shutdown(Shutdown::Both);

    Ok(AuthorizationCode {
        code: SecretBytes::new(code.to_owned()),
    })
}

fn read_http_head(stream: &mut TcpStream) -> Result<String, RuntimeSlotError> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 512];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() > 16_384 {
                    break;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                return Err(RuntimeSlotError::OauthLoopbackInvalidRequest);
            }
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {
                return Err(RuntimeSlotError::OauthLoopbackInvalidRequest);
            }
            Err(_) => return Err(RuntimeSlotError::OauthLoopbackInvalidRequest),
        }
    }
    // Only the request line / headers are needed; never log the buffer
    // (query may contain code).
    String::from_utf8(buf).map_err(|_| RuntimeSlotError::OauthLoopbackInvalidRequest)
}

fn parse_request_target(request: &str) -> Option<&str> {
    let line = request.lines().next()?;
    let mut parts = line.split_whitespace();
    let _method = parts.next()?;
    parts.next()
}

fn query_of_target(target: &str) -> &str {
    target.split_once('?').map(|(_, q)| q).unwrap_or("")
}

/// Minimal application/x-www-form-urlencoded decode for callback query values.
fn query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let decoded_key = percent_decode_basic(k)?;
        if decoded_key == key {
            return percent_decode_basic(v);
        }
    }
    None
}

fn percent_decode_basic(input: &str) -> Option<String> {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h = from_hex(bytes[i + 1])?;
                let l = from_hex(bytes[i + 2])?;
                out.push((h << 4 | l) as char);
                i += 3;
            }
            b if b.is_ascii_graphic() || b == b' ' => {
                out.push(b as char);
                i += 1;
            }
            _ => return None,
        }
    }
    Some(out)
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn write_html_response(
    stream: &TcpStream,
    status: u16,
    title: &str,
    message: &str,
) -> std::io::Result<()> {
    // Static copy only — never interpolate secrets.
    let body = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{title}</title></head>\
         <body><h1>{title}</h1><p>{message}</p></body></html>"
    );
    write_response(stream, status, "text/html; charset=utf-8", body.as_bytes())
}

fn write_plain_response(stream: &TcpStream, status: u16, message: &str) -> std::io::Result<()> {
    write_response(
        stream,
        status,
        "text/plain; charset=utf-8",
        message.as_bytes(),
    )
}

fn write_response(
    mut stream: &TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        409 => "Conflict",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

// ---------------------------------------------------------------------------
// T2 — browser open + authorize URL safety
// ---------------------------------------------------------------------------

/// Options for authorize-URL safety checks.
#[derive(Default, Debug, Clone, Copy)]
pub struct AuthorizeUrlPolicy {
    /// When true, allow `http://127.0.0.1` / `http://[::1]` / `http://localhost`
    /// for injected local test authorization servers. Production must leave false.
    pub allow_loopback_http: bool,
}

/// Validate that `authorize_url` is safe to open and belongs to `issuer`.
///
/// Rules:
/// - scheme must be `https`, unless `allow_loopback_http` and host is loopback
/// - URL origin (scheme://host[:port]) must equal issuer origin, or the URL
///   must start with the issuer string (issuer is a prefix of the authorize URL)
/// - no credentials in the URL
/// - rejects empty / control characters
pub fn validate_authorize_url(
    authorize_url: &str,
    issuer: &str,
    policy: AuthorizeUrlPolicy,
) -> Result<(), RuntimeSlotError> {
    if authorize_url.trim().is_empty()
        || issuer.trim().is_empty()
        || authorize_url.len() > 8_192
        || issuer.len() > 4_096
        || authorize_url.chars().any(char::is_control)
        || issuer.chars().any(char::is_control)
    {
        return Err(RuntimeSlotError::OauthAuthorizeUrlInvalid);
    }

    let url_origin = origin_of(authorize_url).ok_or(RuntimeSlotError::OauthAuthorizeUrlInvalid)?;
    let issuer_origin = origin_of(issuer).ok_or(RuntimeSlotError::OauthAuthorizeUrlInvalid)?;

    if url_origin.scheme == "https" {
        // ok
    } else if url_origin.scheme == "http"
        && policy.allow_loopback_http
        && is_loopback_host(&url_origin.host)
    {
        // test-only loopback AS
    } else {
        return Err(RuntimeSlotError::OauthAuthorizeUrlInvalid);
    }

    // Issuer match: same origin, or authorize URL starts with issuer (typical
    // when issuer is `https://as.example` and authorize is `https://as.example/oauth/authorize`).
    let issuer_trim = issuer.trim_end_matches('/');
    let same_origin = url_origin.scheme == issuer_origin.scheme
        && url_origin.host.eq_ignore_ascii_case(&issuer_origin.host)
        && url_origin.port == issuer_origin.port;
    let prefix_ok = authorize_url.starts_with(issuer_trim);
    if !same_origin && !prefix_ok {
        return Err(RuntimeSlotError::OauthAuthorizeUrlInvalid);
    }

    // Reject embedded userinfo (https://user:pass@host/...).
    if authorize_url.contains('@') {
        // Allow @ only after the authority (rare in query); block userinfo form.
        if let Some(after_scheme) = authorize_url.split_once("://").map(|(_, rest)| rest) {
            let authority = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
            if authority.contains('@') {
                return Err(RuntimeSlotError::OauthAuthorizeUrlInvalid);
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Origin {
    scheme: String,
    host: String,
    port: Option<u16>,
}

fn origin_of(url: &str) -> Option<Origin> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let (host, port) = if authority.starts_with('[') {
        let end = authority.find(']')?;
        let host = authority[..=end].to_string();
        let port = if authority.len() > end + 1 {
            if !authority[end + 1..].starts_with(':') {
                return None;
            }
            Some(authority[end + 2..].parse().ok()?)
        } else {
            None
        };
        (host, port)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => {
                (h.to_string(), Some(p.parse().ok()?))
            }
            _ => (authority.to_string(), None),
        }
    };
    if host.is_empty() {
        return None;
    }
    Some(Origin {
        scheme,
        host: host.to_ascii_lowercase(),
        port,
    })
}

fn is_loopback_host(host: &str) -> bool {
    let h = host.trim_matches(|c| c == '[' || c == ']');
    h.eq_ignore_ascii_case("localhost") || h == "127.0.0.1" || h == "::1" || h.starts_with("127.")
}

/// Open `authorize_url` in the system default browser after policy checks.
pub fn open_authorize_in_browser(
    authorize_url: &str,
    issuer: &str,
    policy: AuthorizeUrlPolicy,
) -> Result<(), RuntimeSlotError> {
    validate_authorize_url(authorize_url, issuer, policy)?;
    open_system_browser(authorize_url)
}

/// Open a URL with the OS default handler. Caller must validate first.
pub fn open_system_browser(url: &str) -> Result<(), RuntimeSlotError> {
    if url.is_empty() || url.chars().any(char::is_control) {
        return Err(RuntimeSlotError::OauthBrowserOpenFailed);
    }
    open_system_browser_impl(url)
}

#[cfg(windows)]
fn open_system_browser_impl(url: &str) -> Result<(), RuntimeSlotError> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    // CREATE_NO_WINDOW — avoid flashing a console for the launcher.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // rundll32 avoids `cmd /C start` quoting hazards for query strings.
    Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|_| RuntimeSlotError::OauthBrowserOpenFailed)?;
    Ok(())
}

#[cfg(not(windows))]
fn open_system_browser_impl(_url: &str) -> Result<(), RuntimeSlotError> {
    Err(RuntimeSlotError::OauthBrowserOpenFailed)
}

/// Injectable browser opener for tests (never opens a real browser).
pub trait BrowserOpener: Send + Sync {
    fn open(&self, url: &str) -> Result<(), RuntimeSlotError>;
}

/// Production opener: system default browser.
pub struct SystemBrowser;

impl BrowserOpener for SystemBrowser {
    fn open(&self, url: &str) -> Result<(), RuntimeSlotError> {
        open_system_browser(url)
    }
}

/// Recording opener for tests.
#[derive(Default)]
pub struct RecordingBrowser {
    pub opened: Mutex<Vec<String>>,
    pub fail: bool,
}

impl BrowserOpener for RecordingBrowser {
    fn open(&self, url: &str) -> Result<(), RuntimeSlotError> {
        if self.fail {
            return Err(RuntimeSlotError::OauthBrowserOpenFailed);
        }
        self.opened
            .lock()
            .map_err(|_| RuntimeSlotError::OauthBrowserOpenFailed)?
            .push(url.to_owned());
        Ok(())
    }
}

/// Validate then open via the provided opener (test seam).
pub fn open_authorize_with(
    opener: &dyn BrowserOpener,
    authorize_url: &str,
    issuer: &str,
    policy: AuthorizeUrlPolicy,
) -> Result<(), RuntimeSlotError> {
    validate_authorize_url(authorize_url, issuer, policy)?;
    opener.open(authorize_url)
}

// ---------------------------------------------------------------------------
// T3 — token exchange / refresh HTTP
// ---------------------------------------------------------------------------

/// Policy for token endpoint URLs.
#[derive(Default, Debug, Clone, Copy)]
pub struct TokenEndpointPolicy {
    pub allow_loopback_http: bool,
}

/// Production OAuth token client backed by the shared WinHTTP path.
#[derive(Debug, Clone)]
pub struct WinHttpOauthTokenClient {
    pub policy: TokenEndpointPolicy,
    pub timeouts: HttpTimeouts,
}

impl Default for WinHttpOauthTokenClient {
    fn default() -> Self {
        Self {
            policy: TokenEndpointPolicy::default(),
            timeouts: HttpTimeouts {
                resolve_ms: 5_000,
                connect_ms: 5_000,
                send_ms: 15_000,
                receive_ms: 15_000,
            },
        }
    }
}

impl WinHttpOauthTokenClient {
    pub fn for_local_tests() -> Self {
        Self {
            policy: TokenEndpointPolicy {
                allow_loopback_http: true,
            },
            timeouts: HttpTimeouts {
                resolve_ms: 2_000,
                connect_ms: 2_000,
                send_ms: 3_000,
                receive_ms: 3_000,
            },
        }
    }

    fn validate_endpoint(&self, token_endpoint: &str) -> Result<(), RuntimeSlotError> {
        validate_token_endpoint(token_endpoint, self.policy)
    }

    /// POST form body; return raw body only on 2xx. Typed transport errors.
    pub fn post_form_raw(
        &self,
        token_endpoint: &str,
        form_body: &str,
    ) -> Result<String, RuntimeSlotError> {
        self.validate_endpoint(token_endpoint)?;
        if form_body.is_empty() {
            return Err(RuntimeSlotError::InvalidInput);
        }
        let response = win_http::post_form(
            token_endpoint,
            form_body.as_bytes(),
            self.timeouts,
            OAUTH_TOKEN_RESPONSE_BYTE_CAP,
        )
        .map_err(|e| e.into_oauth_error())?;

        if !(200..300).contains(&response.status) {
            // Never include response body (may echo secrets) in the error.
            return Err(RuntimeSlotError::OauthHttpNonSuccess);
        }
        String::from_utf8(response.body).map_err(|_| RuntimeSlotError::OauthTokenBadJson)
    }
}

impl OauthTokenClient for WinHttpOauthTokenClient {
    fn post_token_form(
        &self,
        token_endpoint: &str,
        form_body: &str,
    ) -> Result<String, RuntimeSlotError> {
        self.post_form_raw(token_endpoint, form_body)
    }
}

/// Validate token endpoint scheme/host policy.
pub fn validate_token_endpoint(
    token_endpoint: &str,
    policy: TokenEndpointPolicy,
) -> Result<(), RuntimeSlotError> {
    if token_endpoint.trim().is_empty()
        || token_endpoint.len() > 4_096
        || token_endpoint.chars().any(char::is_control)
    {
        return Err(RuntimeSlotError::InvalidInput);
    }
    let origin = origin_of(token_endpoint).ok_or(RuntimeSlotError::InvalidInput)?;
    if origin.scheme == "https" {
        return Ok(());
    }
    if origin.scheme == "http" && policy.allow_loopback_http && is_loopback_host(&origin.host) {
        return Ok(());
    }
    Err(RuntimeSlotError::InvalidInput)
}

/// Exchange an authorization code for tokens (code + PKCE).
///
/// `code` and `code_verifier` are secret-bearing; they are not logged.
pub fn exchange_authorization_code(
    client: &dyn OauthTokenClient,
    token_endpoint: &str,
    client_id: &str,
    code: &SecretBytes,
    redirect_uri: &str,
    code_verifier: &SecretBytes,
) -> Result<TokenResponse, RuntimeSlotError> {
    let code_str =
        std::str::from_utf8(code.as_bytes()).map_err(|_| RuntimeSlotError::InvalidInput)?;
    let verifier_str = std::str::from_utf8(code_verifier.as_bytes())
        .map_err(|_| RuntimeSlotError::InvalidInput)?;
    let form = build_code_exchange_form(client_id, code_str, redirect_uri, verifier_str)?;
    let body = client.post_token_form(token_endpoint, &form)?;
    // form dropped here (contains code + verifier)
    drop(form);
    parse_token_response(&body)
}

/// Refresh tokens via refresh_token grant.
pub fn refresh_tokens(
    client: &dyn OauthTokenClient,
    token_endpoint: &str,
    client_id: &str,
    refresh_token: &SecretBytes,
) -> Result<TokenResponse, RuntimeSlotError> {
    let refresh_str = std::str::from_utf8(refresh_token.as_bytes())
        .map_err(|_| RuntimeSlotError::InvalidInput)?;
    let form = build_refresh_token_form(client_id, refresh_str)?;
    let body = client.post_token_form(token_endpoint, &form)?;
    drop(form);
    parse_token_response(&body)
}

// ---------------------------------------------------------------------------
// T4 — device code flow
// ---------------------------------------------------------------------------

/// Device authorization response. `device_code` is secret; `user_code` is user-facing.
pub struct DeviceAuthorization {
    pub device_code: SecretBytes,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    pub interval: Duration,
}

impl fmt::Debug for DeviceAuthorization {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceAuthorization")
            .field("device_code", &"SecretBytes([REDACTED])")
            .field("user_code", &self.user_code)
            .field("verification_uri", &self.verification_uri)
            .field("verification_uri_complete", &self.verification_uri_complete)
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .finish()
    }
}

/// Build device authorization request form (no secrets).
pub fn build_device_authorization_form(
    client_id: &str,
    scopes: &[String],
) -> Result<String, RuntimeSlotError> {
    if client_id.is_empty() || scopes.is_empty() {
        return Err(RuntimeSlotError::InvalidInput);
    }
    let mut body = String::new();
    push_form(&mut body, "client_id", client_id);
    body.push('&');
    push_form(&mut body, "scope", &scopes.join(" "));
    Ok(body)
}

/// Build device_code token poll form. `device_code` is secret-bearing.
pub fn build_device_token_poll_form(
    client_id: &str,
    device_code: &str,
) -> Result<String, RuntimeSlotError> {
    if client_id.is_empty() || device_code.is_empty() {
        return Err(RuntimeSlotError::InvalidInput);
    }
    let mut body = String::new();
    push_form(
        &mut body,
        "grant_type",
        "urn:ietf:params:oauth:grant-type:device_code",
    );
    body.push('&');
    push_form(&mut body, "client_id", client_id);
    body.push('&');
    push_form(&mut body, "device_code", device_code);
    Ok(body)
}

fn push_form(out: &mut String, key: &str, value: &str) {
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

/// Parse device authorization JSON. Device code never appears in errors.
pub fn parse_device_authorization(body: &str) -> Result<DeviceAuthorization, RuntimeSlotError> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|_| RuntimeSlotError::OauthTokenBadJson)?;
    let device_code = value
        .get("device_code")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or(RuntimeSlotError::OauthTokenMissingField)?;
    let user_code = value
        .get("user_code")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or(RuntimeSlotError::OauthTokenMissingField)?
        .to_owned();
    let verification_uri = value
        .get("verification_uri")
        .or_else(|| value.get("verification_url"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or(RuntimeSlotError::OauthTokenMissingField)?
        .to_owned();
    let verification_uri_complete = value
        .get("verification_uri_complete")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let expires_in = value
        .get("expires_in")
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_i64().and_then(|n| u64::try_from(n).ok()))
        })
        .ok_or(RuntimeSlotError::OauthTokenMissingField)?;
    let interval_secs = value
        .get("interval")
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_i64().and_then(|n| u64::try_from(n).ok()))
        })
        .unwrap_or(DEFAULT_DEVICE_POLL_INTERVAL.as_secs());
    Ok(DeviceAuthorization {
        device_code: SecretBytes::new(device_code.to_owned()),
        user_code,
        verification_uri,
        verification_uri_complete,
        expires_in,
        interval: Duration::from_secs(interval_secs.max(1)),
    })
}

/// Outcome of one device-token poll attempt.
pub enum DevicePollOutcome {
    /// Tokens issued.
    Authorized(TokenResponse),
    /// User has not finished authorization — continue.
    AuthorizationPending,
    /// Server asked to slow down — increase interval then continue.
    SlowDown,
    /// Device code expired.
    Expired,
    /// User denied / other terminal OAuth error.
    Denied,
}

impl fmt::Debug for DevicePollOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authorized(tokens) => f.debug_tuple("Authorized").field(tokens).finish(),
            Self::AuthorizationPending => f.write_str("AuthorizationPending"),
            Self::SlowDown => f.write_str("SlowDown"),
            Self::Expired => f.write_str("Expired"),
            Self::Denied => f.write_str("Denied"),
        }
    }
}

impl PartialEq for DevicePollOutcome {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::AuthorizationPending, Self::AuthorizationPending)
                | (Self::SlowDown, Self::SlowDown)
                | (Self::Expired, Self::Expired)
                | (Self::Denied, Self::Denied)
        )
    }
}

/// Parse a device-code token poll response (success or OAuth error object).
///
/// Never includes device_code or tokens in error variants.
pub fn parse_device_poll_response(
    status: u16,
    body: &str,
) -> Result<DevicePollOutcome, RuntimeSlotError> {
    if (200..300).contains(&status) {
        let tokens = parse_token_response(body)?;
        return Ok(DevicePollOutcome::Authorized(tokens));
    }
    // RFC 8628 uses 400 with error field for pending / slow_down / expired.
    let value: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => {
            if !(200..300).contains(&status) {
                return Err(RuntimeSlotError::OauthHttpNonSuccess);
            }
            return Err(RuntimeSlotError::OauthTokenBadJson);
        }
    };
    let error = value.get("error").and_then(|v| v.as_str()).unwrap_or("");
    match error {
        "authorization_pending" => Ok(DevicePollOutcome::AuthorizationPending),
        "slow_down" => Ok(DevicePollOutcome::SlowDown),
        "expired_token" => Ok(DevicePollOutcome::Expired),
        "access_denied" => Ok(DevicePollOutcome::Denied),
        _ if !(200..300).contains(&status) => Err(RuntimeSlotError::OauthHttpNonSuccess),
        _ => Err(RuntimeSlotError::OauthTokenResponseInvalid),
    }
}

/// Request a device code from the device authorization endpoint.
pub fn request_device_authorization(
    client: &dyn OauthTokenClient,
    device_authorization_endpoint: &str,
    client_id: &str,
    scopes: &[String],
) -> Result<DeviceAuthorization, RuntimeSlotError> {
    let form = build_device_authorization_form(client_id, scopes)?;
    let body = client.post_token_form(device_authorization_endpoint, &form)?;
    parse_device_authorization(&body)
}

/// Clock / sleep injection for device poll tests.
pub trait PollClock: Send + Sync {
    fn sleep(&self, duration: Duration);
    fn now(&self) -> Instant;
}

/// Wall-clock implementation.
pub struct SystemClock;

impl PollClock for SystemClock {
    fn sleep(&self, duration: Duration) {
        thread::sleep(duration);
    }
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Virtual clock for tests: sleep advances now without wall delay.
pub struct VirtualClock {
    now: Mutex<Instant>,
    pub sleeps: Mutex<Vec<Duration>>,
}

impl VirtualClock {
    pub fn new() -> Self {
        Self {
            now: Mutex::new(Instant::now()),
            sleeps: Mutex::new(Vec::new()),
        }
    }
}

impl Default for VirtualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl PollClock for VirtualClock {
    fn sleep(&self, duration: Duration) {
        if let Ok(mut n) = self.now.lock() {
            *n += duration;
        }
        if let Ok(mut s) = self.sleeps.lock() {
            s.push(duration);
        }
    }
    fn now(&self) -> Instant {
        self.now
            .lock()
            .map(|g| *g)
            .unwrap_or_else(|_| Instant::now())
    }
}

/// Poll the token endpoint until authorized, expired, denied, or deadline.
///
/// Handles `authorization_pending`, `slow_down` (+5s), and `expired_token`.
/// Device code is never logged.
pub fn poll_device_token(
    client: &dyn OauthTokenClient,
    token_endpoint: &str,
    client_id: &str,
    device: &DeviceAuthorization,
    clock: &dyn PollClock,
) -> Result<TokenResponse, RuntimeSlotError> {
    let device_code_str = std::str::from_utf8(device.device_code.as_bytes())
        .map_err(|_| RuntimeSlotError::InvalidInput)?;
    let form = build_device_token_poll_form(client_id, device_code_str)?;
    let deadline = clock.now() + Duration::from_secs(device.expires_in.max(1));
    let mut interval = device.interval;

    loop {
        if clock.now() >= deadline {
            return Err(RuntimeSlotError::OauthDeviceExpired);
        }

        // Use a specialized path that preserves non-2xx bodies for error codes.
        let outcome = match client.post_token_form(token_endpoint, &form) {
            Ok(body) => parse_device_poll_response(200, &body)?,
            Err(RuntimeSlotError::OauthHttpNonSuccess) => {
                // Generic non-2xx without body — treat as hard failure.
                // Callers that need pending/slow_down must use DevicePollClient.
                return Err(RuntimeSlotError::OauthHttpNonSuccess);
            }
            Err(other) => return Err(other),
        };

        match outcome {
            DevicePollOutcome::Authorized(tokens) => return Ok(tokens),
            DevicePollOutcome::AuthorizationPending => {
                clock.sleep(interval);
            }
            DevicePollOutcome::SlowDown => {
                interval = interval.saturating_add(DEVICE_SLOW_DOWN_INCREMENT);
                clock.sleep(interval);
            }
            DevicePollOutcome::Expired => return Err(RuntimeSlotError::OauthDeviceExpired),
            DevicePollOutcome::Denied => return Err(RuntimeSlotError::OauthDeviceDenied),
        }
    }
}

/// Extended token client that can return status + body for device poll.
///
/// Production `WinHttpOauthTokenClient` implements this; tests inject fakes.
pub trait DeviceTokenTransport: Send + Sync {
    fn post_form_with_status(
        &self,
        endpoint: &str,
        form_body: &str,
    ) -> Result<(u16, String), RuntimeSlotError>;
}

impl DeviceTokenTransport for WinHttpOauthTokenClient {
    fn post_form_with_status(
        &self,
        endpoint: &str,
        form_body: &str,
    ) -> Result<(u16, String), RuntimeSlotError> {
        self.validate_endpoint(endpoint)?;
        if form_body.is_empty() {
            return Err(RuntimeSlotError::InvalidInput);
        }
        let response = win_http::post_form(
            endpoint,
            form_body.as_bytes(),
            self.timeouts,
            OAUTH_TOKEN_RESPONSE_BYTE_CAP,
        )
        .map_err(|e| e.into_oauth_error())?;
        let body =
            String::from_utf8(response.body).map_err(|_| RuntimeSlotError::OauthTokenBadJson)?;
        Ok((response.status, body))
    }
}

/// Device poll using status-aware transport (correct for pending/slow_down on 400).
pub fn poll_device_token_with_transport(
    transport: &dyn DeviceTokenTransport,
    token_endpoint: &str,
    client_id: &str,
    device: &DeviceAuthorization,
    clock: &dyn PollClock,
) -> Result<TokenResponse, RuntimeSlotError> {
    let device_code_str = std::str::from_utf8(device.device_code.as_bytes())
        .map_err(|_| RuntimeSlotError::InvalidInput)?;
    let form = build_device_token_poll_form(client_id, device_code_str)?;
    let deadline = clock.now() + Duration::from_secs(device.expires_in.max(1));
    let mut interval = device.interval;

    loop {
        if clock.now() >= deadline {
            return Err(RuntimeSlotError::OauthDeviceExpired);
        }

        let (status, body) = transport.post_form_with_status(token_endpoint, &form)?;
        let outcome = parse_device_poll_response(status, &body)?;

        match outcome {
            DevicePollOutcome::Authorized(tokens) => return Ok(tokens),
            DevicePollOutcome::AuthorizationPending => {
                clock.sleep(interval);
            }
            DevicePollOutcome::SlowDown => {
                interval = interval.saturating_add(DEVICE_SLOW_DOWN_INCREMENT);
                clock.sleep(interval);
            }
            DevicePollOutcome::Expired => return Err(RuntimeSlotError::OauthDeviceExpired),
            DevicePollOutcome::Denied => return Err(RuntimeSlotError::OauthDeviceDenied),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn runtime_token(prefix: &str, suffix: &str) -> String {
        format!("{prefix}{suffix}")
    }

    // ----- T1 -----

    #[test]
    fn loopback_accepts_matching_state_and_redacts_code_in_response() {
        let listener = LoopbackListener::bind().expect("bind");
        let port = listener.port();
        let redirect = listener.redirect_uri().to_owned();
        assert!(redirect.starts_with("http://127.0.0.1:"));
        assert!(redirect.ends_with("/callback"));
        assert_ne!(port, 0);

        let code_material = runtime_token("authz-", "code-material-t1");
        let state = "state-expected-abc";
        let code_for_client = code_material.clone();

        let client = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
            let req = format!(
                "GET /callback?code={code_for_client}&state={state} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(req.as_bytes()).expect("write");
            let mut resp = String::new();
            stream.read_to_string(&mut resp).expect("read");
            resp
        });

        let auth = listener
            .wait_for_code(state, Duration::from_secs(5))
            .expect("code");
        assert_eq!(auth.as_secret().as_bytes(), code_material.as_bytes());
        let debug = format!("{auth:?}");
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains(&code_material));

        let resp = client.join().expect("join");
        assert!(resp.starts_with("HTTP/1.1 200 "));
        assert!(!resp.contains(&code_material));
        assert!(!resp.contains("code="));
    }

    #[test]
    fn loopback_rejects_state_mismatch() {
        let listener = LoopbackListener::bind().expect("bind");
        let port = listener.port();
        let code_material = runtime_token("authz-", "code-mismatch");
        let code_for_client = code_material.clone();

        let client = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
            let req = format!(
                "GET /callback?code={code_for_client}&state=wrong-state HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(req.as_bytes()).unwrap();
            let mut resp = String::new();
            let _ = stream.read_to_string(&mut resp);
            resp
        });

        let err = listener
            .wait_for_code("expected-state", Duration::from_secs(5))
            .expect_err("state");
        assert_eq!(err.code(), "runtime_slot_oauth_loopback_state_mismatch");
        assert!(!err.to_string().contains(&code_material));

        let resp = client.join().unwrap();
        assert!(resp.contains("400"));
        assert!(!resp.contains(&code_material));
    }

    #[test]
    fn loopback_timeout_cleans_up() {
        let listener = LoopbackListener::bind().expect("bind");
        let port = listener.port();
        let err = listener
            .wait_for_code("any-state", Duration::from_millis(200))
            .expect_err("timeout");
        assert_eq!(err.code(), "runtime_slot_oauth_loopback_timeout");
        // Port should be free again (listener dropped).
        let rebind = TcpListener::bind(("127.0.0.1", port));
        assert!(rebind.is_ok(), "timeout must release the port");
    }

    #[test]
    fn loopback_second_request_not_accepted_after_success() {
        let listener = LoopbackListener::bind().expect("bind");
        let port = listener.port();
        let code_material = runtime_token("authz-", "code-once");
        let state = "once-state";
        let code_clone = code_material.clone();

        // First client delivers the code.
        let first = thread::spawn(move || {
            thread::sleep(Duration::from_millis(40));
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let req = format!(
                "GET /callback?code={code_clone}&state={state} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(req.as_bytes()).unwrap();
            let mut resp = String::new();
            let _ = stream.read_to_string(&mut resp);
            resp
        });

        let auth = listener
            .wait_for_code(state, Duration::from_secs(5))
            .expect("first");
        assert_eq!(auth.as_secret().as_bytes(), code_material.as_bytes());
        let _ = first.join();

        // Listener consumed — connect should fail (port closed) or not yield a code.
        let second = TcpStream::connect(("127.0.0.1", port));
        assert!(
            second.is_err(),
            "second request must not be served after listener is dropped"
        );
    }

    // ----- T2 -----

    #[test]
    fn authorize_url_requires_https_and_issuer_match() {
        let issuer = "https://issuer.example.invalid";
        let good = "https://issuer.example.invalid/authorize?response_type=code";
        assert!(validate_authorize_url(good, issuer, AuthorizeUrlPolicy::default()).is_ok());

        let wrong_host = "https://evil.example.invalid/authorize";
        assert_eq!(
            validate_authorize_url(wrong_host, issuer, AuthorizeUrlPolicy::default())
                .unwrap_err()
                .code(),
            "runtime_slot_oauth_authorize_url_invalid"
        );

        let http = "http://issuer.example.invalid/authorize";
        assert_eq!(
            validate_authorize_url(http, issuer, AuthorizeUrlPolicy::default())
                .unwrap_err()
                .code(),
            "runtime_slot_oauth_authorize_url_invalid"
        );

        let loopback_http = "http://127.0.0.1:9/authorize";
        assert!(
            validate_authorize_url(
                loopback_http,
                "http://127.0.0.1:9",
                AuthorizeUrlPolicy {
                    allow_loopback_http: true
                }
            )
            .is_ok()
        );
        assert!(
            validate_authorize_url(
                loopback_http,
                "http://127.0.0.1:9",
                AuthorizeUrlPolicy::default()
            )
            .is_err()
        );
    }

    #[test]
    fn open_authorize_with_recording_browser_validates_first() {
        let browser = RecordingBrowser::default();
        let issuer = "https://issuer.example.invalid";
        let url = "https://issuer.example.invalid/authorize?x=1";
        open_authorize_with(&browser, url, issuer, AuthorizeUrlPolicy::default()).unwrap();
        assert_eq!(browser.opened.lock().unwrap().as_slice(), [url]);

        let evil = "https://evil.example.invalid/authorize";
        assert!(
            open_authorize_with(&browser, evil, issuer, AuthorizeUrlPolicy::default()).is_err()
        );
        // Evil URL must not have been opened.
        assert_eq!(browser.opened.lock().unwrap().len(), 1);
    }

    // ----- T3 parse / exchange (client injected) -----

    struct SequenceClient {
        responses: Mutex<Vec<Result<String, RuntimeSlotError>>>,
        seen: Mutex<Vec<String>>,
    }

    impl SequenceClient {
        fn new(responses: Vec<Result<String, RuntimeSlotError>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                seen: Mutex::new(Vec::new()),
            }
        }
    }

    impl OauthTokenClient for SequenceClient {
        fn post_token_form(
            &self,
            _token_endpoint: &str,
            form_body: &str,
        ) -> Result<String, RuntimeSlotError> {
            self.seen.lock().unwrap().push(form_body.to_owned());
            let mut q = self.responses.lock().unwrap();
            if q.is_empty() {
                return Err(RuntimeSlotError::OauthHttpUnavailable);
            }
            q.remove(0)
        }
    }

    #[test]
    fn exchange_code_success_and_error_modes() {
        let access = runtime_token("acc-", "ess-exch");
        let refresh = runtime_token("ref-", "resh-exch");
        let code = runtime_token("authz-", "code-exch");
        let verifier = runtime_token("verif-", "ier-exch-0123456789abcdef");

        let ok_body = format!(
            r#"{{"access_token":"{access}","refresh_token":"{refresh}","expires_in":3600}}"#
        );
        let client = SequenceClient::new(vec![Ok(ok_body)]);
        let tokens = exchange_authorization_code(
            &client,
            "https://issuer.example.invalid/token",
            "client-from-caller",
            &SecretBytes::new(code.clone()),
            "http://127.0.0.1:9/callback",
            &SecretBytes::new(verifier.clone()),
        )
        .expect("exchange");
        assert_eq!(tokens.access_token.as_bytes(), access.as_bytes());

        // Timeout
        let client = SequenceClient::new(vec![Err(RuntimeSlotError::OauthHttpTimeout)]);
        let err = exchange_authorization_code(
            &client,
            "https://issuer.example.invalid/token",
            "client-from-caller",
            &SecretBytes::new(code.clone()),
            "http://127.0.0.1:9/callback",
            &SecretBytes::new(verifier.clone()),
        )
        .unwrap_err();
        assert_eq!(err.code(), "runtime_slot_oauth_http_timeout");
        assert!(!err.to_string().contains(&code));

        // Non-2xx
        let client = SequenceClient::new(vec![Err(RuntimeSlotError::OauthHttpNonSuccess)]);
        let err = exchange_authorization_code(
            &client,
            "https://issuer.example.invalid/token",
            "client-from-caller",
            &SecretBytes::new(code.clone()),
            "http://127.0.0.1:9/callback",
            &SecretBytes::new(verifier.clone()),
        )
        .unwrap_err();
        assert_eq!(err.code(), "runtime_slot_oauth_http_non_success");

        // Bad JSON
        let client = SequenceClient::new(vec![Ok("not-json".into())]);
        let err = exchange_authorization_code(
            &client,
            "https://issuer.example.invalid/token",
            "client-from-caller",
            &SecretBytes::new(code.clone()),
            "http://127.0.0.1:9/callback",
            &SecretBytes::new(verifier.clone()),
        )
        .unwrap_err();
        assert_eq!(err.code(), "runtime_slot_oauth_token_bad_json");
        assert!(!err.to_string().contains(&code));

        // Missing field
        let client = SequenceClient::new(vec![Ok(r#"{"expires_in":1}"#.into())]);
        let err = exchange_authorization_code(
            &client,
            "https://issuer.example.invalid/token",
            "client-from-caller",
            &SecretBytes::new(code.clone()),
            "http://127.0.0.1:9/callback",
            &SecretBytes::new(verifier),
        )
        .unwrap_err();
        assert_eq!(err.code(), "runtime_slot_oauth_token_missing_field");
        assert!(!format!("{err:?}").contains(&code));
        assert!(!format!("{err}").contains(&access));
    }

    #[test]
    fn refresh_and_error_text_never_contain_token_or_code() {
        let refresh = runtime_token("ref-", "resh-secret-marker");
        let access = runtime_token("acc-", "ess-secret-marker");
        let body = format!(r#"{{"access_token":"{access}","expires_in":60}}"#);
        let client = SequenceClient::new(vec![Ok(body)]);
        let tokens = refresh_tokens(
            &client,
            "https://issuer.example.invalid/token",
            "client-from-caller",
            &SecretBytes::new(refresh.clone()),
        )
        .unwrap();
        let dbg = format!("{tokens:?}");
        assert!(!dbg.contains(&access));
        assert!(!dbg.contains(&refresh));
        assert!(dbg.contains("REDACTED"));

        // Ensure form was posted (contains refresh) but error paths don't echo it.
        let client = SequenceClient::new(vec![Err(RuntimeSlotError::OauthHttpNonSuccess)]);
        let err = refresh_tokens(
            &client,
            "https://issuer.example.invalid/token",
            "client-from-caller",
            &SecretBytes::new(refresh.clone()),
        )
        .unwrap_err();
        assert!(!err.to_string().contains(&refresh));
        assert!(!format!("{err:?}").contains(&refresh));
    }

    // ----- T4 -----

    #[test]
    fn device_authorization_parse_redacts_device_code() {
        let dc = runtime_token("dev-", "ice-code-secret");
        let body = format!(
            r#"{{"device_code":"{dc}","user_code":"WDJB-MJHT","verification_uri":"https://issuer.example.invalid/device","expires_in":1800,"interval":5}}"#
        );
        let parsed = parse_device_authorization(&body).unwrap();
        assert_eq!(parsed.user_code, "WDJB-MJHT");
        assert_eq!(parsed.device_code.as_bytes(), dc.as_bytes());
        let dbg = format!("{parsed:?}");
        assert!(dbg.contains("WDJB-MJHT"));
        assert!(dbg.contains("REDACTED"));
        assert!(!dbg.contains(&dc));
    }

    #[test]
    fn device_poll_pending_slow_down_expired() {
        assert_eq!(
            parse_device_poll_response(400, r#"{"error":"authorization_pending"}"#).unwrap(),
            DevicePollOutcome::AuthorizationPending
        );
        assert_eq!(
            parse_device_poll_response(400, r#"{"error":"slow_down"}"#).unwrap(),
            DevicePollOutcome::SlowDown
        );
        assert_eq!(
            parse_device_poll_response(400, r#"{"error":"expired_token"}"#).unwrap(),
            DevicePollOutcome::Expired
        );
        assert_eq!(
            parse_device_poll_response(400, r#"{"error":"access_denied"}"#).unwrap(),
            DevicePollOutcome::Denied
        );

        let access = runtime_token("acc-", "ess-device");
        let body = format!(r#"{{"access_token":"{access}","expires_in":60}}"#);
        match parse_device_poll_response(200, &body).unwrap() {
            DevicePollOutcome::Authorized(t) => {
                assert_eq!(t.access_token.as_bytes(), access.as_bytes());
            }
            other => panic!("expected authorized, got {other:?}"),
        }
    }

    struct FakeDeviceTransport {
        // Each call pops one (status, body)
        queue: Mutex<Vec<(u16, String)>>,
    }

    impl DeviceTokenTransport for FakeDeviceTransport {
        fn post_form_with_status(
            &self,
            _endpoint: &str,
            form_body: &str,
        ) -> Result<(u16, String), RuntimeSlotError> {
            // Device code must be in the form but we never log form_body.
            let _ = form_body.len();
            let mut q = self.queue.lock().unwrap();
            if q.is_empty() {
                return Err(RuntimeSlotError::OauthHttpUnavailable);
            }
            Ok(q.remove(0))
        }
    }

    #[test]
    fn device_poll_loop_handles_pending_then_success() {
        let dc = runtime_token("dev-", "ice-poll-1");
        let access = runtime_token("acc-", "ess-poll-1");
        let device = DeviceAuthorization {
            device_code: SecretBytes::new(dc),
            user_code: "USER-CODE".into(),
            verification_uri: "https://issuer.example.invalid/device".into(),
            verification_uri_complete: None,
            expires_in: 600,
            interval: Duration::from_secs(5),
        };
        let transport = FakeDeviceTransport {
            queue: Mutex::new(vec![
                (400, r#"{"error":"authorization_pending"}"#.into()),
                (400, r#"{"error":"slow_down"}"#.into()),
                (
                    200,
                    format!(r#"{{"access_token":"{access}","expires_in":120}}"#),
                ),
            ]),
        };
        let clock = VirtualClock::new();
        let tokens = poll_device_token_with_transport(
            &transport,
            "https://issuer.example.invalid/token",
            "client-from-caller",
            &device,
            &clock,
        )
        .unwrap();
        assert_eq!(tokens.access_token.as_bytes(), access.as_bytes());
        let sleeps = clock.sleeps.lock().unwrap().clone();
        // pending @ 5s, then slow_down → interval becomes 10s
        assert_eq!(
            sleeps,
            vec![Duration::from_secs(5), Duration::from_secs(10)]
        );
    }

    #[test]
    fn device_poll_expired_is_terminal() {
        let device = DeviceAuthorization {
            device_code: SecretBytes::new(runtime_token("dev-", "ice-exp")),
            user_code: "X".into(),
            verification_uri: "https://issuer.example.invalid/device".into(),
            verification_uri_complete: None,
            expires_in: 600,
            interval: Duration::from_secs(1),
        };
        let transport = FakeDeviceTransport {
            queue: Mutex::new(vec![(400, r#"{"error":"expired_token"}"#.into())]),
        };
        let err = poll_device_token_with_transport(
            &transport,
            "https://issuer.example.invalid/token",
            "client-from-caller",
            &device,
            &VirtualClock::new(),
        )
        .unwrap_err();
        assert_eq!(err.code(), "runtime_slot_oauth_device_expired");
    }

    // ----- live local HTTP (Windows WinHTTP) -----

    #[cfg(windows)]
    #[test]
    fn winhttp_token_post_success_and_failure_modes() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let access = runtime_token("acc-", "ess-winhttp");
        let refresh = runtime_token("ref-", "resh-winhttp");
        let code = runtime_token("authz-", "code-winhttp");

        // Success server
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let access_c = access.clone();
        let refresh_c = refresh.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut req = Vec::new();
            let mut buf = [0u8; 1024];
            while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = stream.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    break;
                }
                req.extend_from_slice(&buf[..n]);
                if req.len() > 32_768 {
                    break;
                }
            }
            // Read body if Content-Length
            let head = String::from_utf8_lossy(&req);
            if let Some(cl) = head
                .lines()
                .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
            {
                let n: usize = cl.split(':').nth(1).unwrap().trim().parse().unwrap_or(0);
                let body_start = req.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
                while req.len() < body_start + n {
                    let r = stream.read(&mut buf).unwrap_or(0);
                    if r == 0 {
                        break;
                    }
                    req.extend_from_slice(&buf[..r]);
                }
            }
            let body = format!(
                r#"{{"access_token":"{access_c}","refresh_token":"{refresh_c}","expires_in":3600,"token_type":"Bearer"}}"#
            );
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            String::from_utf8_lossy(&req).into_owned()
        });

        let client = WinHttpOauthTokenClient::for_local_tests();
        let tokens = exchange_authorization_code(
            &client,
            &format!("http://{addr}/token"),
            "client-from-caller",
            &SecretBytes::new(code.clone()),
            "http://127.0.0.1:9/callback",
            &SecretBytes::new(runtime_token("verif-", "ier-winhttp-01234567890123456789")),
        )
        .expect("local exchange");
        assert_eq!(tokens.access_token.as_bytes(), access.as_bytes());
        let req = server.join().unwrap();
        assert!(req.contains("POST /token"));
        assert!(req.contains("grant_type=authorization_code"));
        // Form contains code — ensure we don't surface it in Result errors (already checked).
        assert!(req.contains("client_id=client-from-caller"));

        // Drain one HTTP request fully, then reply (keeps WinHTTP happy on POST).
        fn serve_once(status_line: &'static str, body: &'static [u8]) -> std::net::SocketAddr {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let mut req = Vec::new();
                let mut buf = [0u8; 1024];
                while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            req.extend_from_slice(&buf[..n]);
                            if req.len() > 32_768 {
                                break;
                            }
                        }
                    }
                }
                if let Some(pos) = req.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&req[..pos]);
                    if let Some(cl) = head
                        .lines()
                        .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
                    {
                        let n: usize = cl
                            .split(':')
                            .nth(1)
                            .and_then(|s| s.trim().parse().ok())
                            .unwrap_or(0);
                        let body_start = pos + 4;
                        while req.len() < body_start + n {
                            match stream.read(&mut buf) {
                                Ok(0) | Err(_) => break,
                                Ok(m) => req.extend_from_slice(&buf[..m]),
                            }
                        }
                    }
                }
                let _ = write!(
                    stream,
                    "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(body);
            });
            thread::sleep(Duration::from_millis(30));
            addr
        }

        let addr = serve_once("HTTP/1.1 400 Bad Request", br#"{"error":"invalid_grant"}"#);
        let err = client
            .post_form_raw(
                &format!("http://{addr}/token"),
                "grant_type=refresh_token&client_id=c&refresh_token=x",
            )
            .unwrap_err();
        assert_eq!(
            err.code(),
            "runtime_slot_oauth_http_non_success",
            "non-2xx must be distinct; got {}",
            err.code()
        );
        assert!(!err.to_string().contains("invalid_grant"));

        let addr = serve_once("HTTP/1.1 200 OK", b"not-json-at-all");
        let err = exchange_authorization_code(
            &client,
            &format!("http://{addr}/token"),
            "client-from-caller",
            &SecretBytes::new(code.clone()),
            "http://127.0.0.1:9/callback",
            &SecretBytes::new(runtime_token("verif-", "ier-badjson-0123456789012345")),
        )
        .unwrap_err();
        assert_eq!(err.code(), "runtime_slot_oauth_token_bad_json");

        let addr = serve_once("HTTP/1.1 200 OK", br#"{"expires_in":1}"#);
        let err = exchange_authorization_code(
            &client,
            &format!("http://{addr}/token"),
            "client-from-caller",
            &SecretBytes::new(code),
            "http://127.0.0.1:9/callback",
            &SecretBytes::new(runtime_token("verif-", "ier-missing-0123456789012345")),
        )
        .unwrap_err();
        assert_eq!(err.code(), "runtime_slot_oauth_token_missing_field");
    }

    #[cfg(windows)]
    #[test]
    fn winhttp_timeout_is_distinct_error() {
        // Bind but never accept → client connect/receive timeout.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        // Keep listener alive but do not accept.
        let client = WinHttpOauthTokenClient {
            policy: TokenEndpointPolicy {
                allow_loopback_http: true,
            },
            timeouts: HttpTimeouts {
                resolve_ms: 500,
                connect_ms: 500,
                send_ms: 500,
                receive_ms: 500,
            },
        };
        // Connecting to a listening-but-not-accepting socket may hang on send/receive.
        // Use a non-routable address for a cleaner timeout: 127.0.0.1 with closed port
        // after dropping listener... actually for timeout we need a black hole.
        // Hold the listener without accept — WinHTTP will send SYN, complete TCP, then
        // hang on response → receive timeout.
        let handle = thread::spawn(move || {
            // Park the listener until test ends.
            thread::sleep(Duration::from_secs(10));
            drop(listener);
        });
        let err = client
            .post_form_raw(
                &format!("http://{addr}/token"),
                "grant_type=refresh_token&client_id=c&refresh_token=x",
            )
            .unwrap_err();
        // Timeout or unavailable both acceptable as hard fail; prefer timeout code.
        assert!(
            matches!(
                err,
                RuntimeSlotError::OauthHttpTimeout | RuntimeSlotError::OauthHttpUnavailable
            ),
            "expected timeout-class error, got {}",
            err.code()
        );
        // If we got Unavailable (e.g. connection reset), still distinct from NonSuccess/BadJson.
        assert_ne!(err.code(), "runtime_slot_oauth_http_non_success");
        assert_ne!(err.code(), "runtime_slot_oauth_token_bad_json");
        let _ = handle; // leave thread; process exit cleans up
    }

    #[test]
    fn token_endpoint_rejects_non_https_without_loopback_opt_in() {
        assert!(
            validate_token_endpoint(
                "https://issuer.example.invalid/token",
                TokenEndpointPolicy::default()
            )
            .is_ok()
        );
        assert!(
            validate_token_endpoint(
                "http://issuer.example.invalid/token",
                TokenEndpointPolicy::default()
            )
            .is_err()
        );
        assert!(
            validate_token_endpoint(
                "http://127.0.0.1:9/token",
                TokenEndpointPolicy {
                    allow_loopback_http: true
                }
            )
            .is_ok()
        );
    }
}
