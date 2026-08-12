# OAuth transport report — 2026-08-12

Worker: grok1. Total control: Claude. Crate: `pillow-runtime-slots`.

## Status

**COMPLETE**

Completion signal: **`OAUTH-TRANSPORT-COMPLETE`**

## Scope

| Allowed | Forbidden |
| --- | --- |
| `crates/runtime-slots/**` | `apps/desktop/**` |
| This report | `crates/knowledge-indexer/**` |
| | root `Cargo.toml` |
| | git write ops |
| | Real vendor endpoints |
| | Real auth files (never read/print/write) |

## Safety posture (honored)

1. Authorization Code + PKCE and Device Code only (OAuth 2.1 / RFC 8628).
2. No vendor-proprietary flows; **no hard-coded client_id / secret / issuer**.
3. All network tests hit **local loopback servers** only (`127.0.0.1`).
4. Tokens / authorization codes / device codes never in logs, errors, Debug, or DTOs.
5. Fail closed — transport errors are typed; prepare path still refuses expired OAuth without a successful refresh.

---

## What landed

| Area | Module / API |
| --- | --- |
| Shared WinHTTP | `src/win_http.rs` — single client for catalog GET + OAuth POST |
| T1 loopback | `LoopbackListener::bind` / `wait_for_code` — `127.0.0.1` only, random port, state check, hard timeout, one-shot; success HTML **never echoes code** |
| T2 browser | `validate_authorize_url` + `open_authorize_in_browser` / `open_authorize_with`; https required unless `allow_loopback_http`; issuer origin/prefix match |
| T3 token HTTP | `WinHttpOauthTokenClient`, `exchange_authorization_code`, `refresh_tokens`; typed errors for timeout / non-2xx / bad JSON / missing field |
| T4 device code | `parse_device_authorization`, `parse_device_poll_response`, `poll_device_token_with_transport`; pending / slow_down (+5s) / expired_token |
| Pure helpers (prior) | `oauth.rs` — PKCE, forms, parse (now splits bad JSON vs missing field) |

### New error codes (Display never includes secrets)

| Variant | `code()` |
| --- | --- |
| `OauthTokenBadJson` | `runtime_slot_oauth_token_bad_json` |
| `OauthTokenMissingField` | `runtime_slot_oauth_token_missing_field` |
| `OauthHttpTimeout` | `runtime_slot_oauth_http_timeout` |
| `OauthHttpUnavailable` | `runtime_slot_oauth_http_unavailable` |
| `OauthHttpNonSuccess` | `runtime_slot_oauth_http_non_success` |
| `OauthLoopbackBindFailed` | `runtime_slot_oauth_loopback_bind_failed` |
| `OauthLoopbackTimeout` | `runtime_slot_oauth_loopback_timeout` |
| `OauthLoopbackStateMismatch` | `runtime_slot_oauth_loopback_state_mismatch` |
| `OauthLoopbackAlreadyConsumed` | `runtime_slot_oauth_loopback_already_consumed` |
| `OauthLoopbackInvalidRequest` | `runtime_slot_oauth_loopback_invalid_request` |
| `OauthLoopbackDenied` | `runtime_slot_oauth_loopback_denied` |
| `OauthAuthorizeUrlInvalid` | `runtime_slot_oauth_authorize_url_invalid` |
| `OauthBrowserOpenFailed` | `runtime_slot_oauth_browser_open_failed` |
| `OauthDeviceExpired` | `runtime_slot_oauth_device_expired` |
| `OauthDeviceDenied` | `runtime_slot_oauth_device_denied` |

Existing: `OauthRefreshFailed`, `OauthTokenResponseInvalid` retained.

---

## 1. Exit code table (actually run)

Environment: Windows, toolchain via an explicit `RUSTUP_HOME` pointing at the host rustup home (the sandbox default had no toolchains). Not product code; the concrete path is omitted — reports must not carry host user paths.

| Command | Exit | Notes |
| --- | --- | --- |
| `cargo test -p pillow-runtime-slots --lib` | **0** | `31 passed; 0 failed` (~4s) |
| `cargo test -p pillow-runtime-slots` | **0** | lib 31 + integration `slot_service` 16 + doc 0 |

Capture style (PowerShell):  
`cargo test -p pillow-runtime-slots > log 2>&1; echo EXIT=$LASTEXITCODE`  
(not piped through `tail`; exit taken from `$LASTEXITCODE` / process exit).

---

## 2. T1 evidence (loopback)

| Case | Test | Evidence |
| --- | --- | --- |
| Matching `state` + code redacted in HTML | `loopback_accepts_matching_state_and_redacts_code_in_response` | 200 response; body has no `code=` and no code material; `AuthorizationCode` Debug is `REDACTED` |
| `state` mismatch rejected | `loopback_rejects_state_mismatch` | `runtime_slot_oauth_loopback_state_mismatch`; 400 page; error/`Display` has no code material |
| Timeout cleans up | `loopback_timeout_cleans_up` | `runtime_slot_oauth_loopback_timeout`; port rebinds successfully after return |
| Second request not accepted | `loopback_second_request_not_accepted_after_success` | After first success, listener is consumed; connect to same port fails |

Bind rules: `TcpListener::bind(("127.0.0.1", 0))` only — never `0.0.0.0`.

---

## 3. T3 failure modes (independent errors)

| Mode | Error code | Test |
| --- | --- | --- |
| Timeout | `runtime_slot_oauth_http_timeout` (or unavailable class; never non-success/bad-json) | `exchange_code_success_and_error_modes` (injected); `winhttp_timeout_is_distinct_error` (live loopback hang) |
| Non-2xx | `runtime_slot_oauth_http_non_success` | injected + `winhttp_token_post_success_and_failure_modes` (local 400) |
| Bad JSON | `runtime_slot_oauth_token_bad_json` | injected + live 200 `not-json-at-all` |
| Missing field | `runtime_slot_oauth_token_missing_field` | injected + live 200 `{"expires_in":1}` |

Live HTTP uses **only** `http://127.0.0.1:*` with `TokenEndpointPolicy { allow_loopback_http: true }`. Production default rejects non-https.

---

## 4. Secrets not in errors / logs / Debug

| Test | Assertion |
| --- | --- |
| `token_response_debug_redacts_secrets` (oauth.rs) | Debug contains `REDACTED`, not access/refresh material |
| `refresh_and_error_text_never_contain_token_or_code` | TokenResponse Debug + refresh error paths lack refresh/access material |
| `exchange_code_success_and_error_modes` | Timeout/bad-json/missing-field errors lack authorization code |
| `loopback_*` | Response HTML and error strings lack code material |
| `device_authorization_parse_redacts_device_code` | Debug shows `user_code`, redacts `device_code` |
| WinHTTP non-2xx | Error does not contain `invalid_grant` response body |

No logging of form bodies, codes, or tokens was added.

---

## 5. T4 device code evidence

| Server state | Handling | Test |
| --- | --- | --- |
| `authorization_pending` | Continue poll at current interval | `device_poll_pending_slow_down_expired` + `device_poll_loop_handles_pending_then_success` |
| `slow_down` | Interval += 5s, then sleep | loop test: sleeps `[5s, 10s]` then tokens |
| `expired_token` | Terminal `runtime_slot_oauth_device_expired` | `device_poll_expired_is_terminal` |

`user_code` is public for display; `device_code` is `SecretBytes` only.

---

## 6. Dependencies for total-control

**None required.** No new crates. Reused:

- Existing workspace: `serde_json`, `base64`, `sha2`, `getrandom`, `thiserror`, …
- Existing crate `windows-sys` features: `Win32_Foundation`, `Win32_Networking_WinHttp`, `Win32_Security_Cryptography`
- No root `Cargo.toml` edits

Optional later (not needed now): if total-control wants ShellExecute instead of `rundll32 url.dll,FileProtocolHandler`, add `Win32_UI_Shell` feature — not required for this ticket.

---

## 7. Write-boundary audit

### Touched

| Path | Change |
| --- | --- |
| `crates/runtime-slots/src/win_http.rs` | **New** shared WinHTTP GET/POST |
| `crates/runtime-slots/src/oauth_transport.rs` | **New** T1–T4 transport + tests |
| `crates/runtime-slots/src/catalog.rs` | Catalog GET now calls `win_http` (one client) |
| `crates/runtime-slots/src/oauth.rs` | Parse splits bad JSON vs missing field |
| `crates/runtime-slots/src/lib.rs` | Module wire-up, new errors, re-exports |
| `docs/execution/2026-08-12-oauth-transport-report.md` | This report |

### Not touched

- `apps/desktop/**`, `crates/knowledge-indexer/**`, root `Cargo.toml`
- Git write operations
- Real auth files (including `~/.local/share/opencode/auth.json`)
- No vendor client_id / secret literals
- No requests to real AS/token endpoints

### Residual risks — **where a user account could still get hurt**

1. **Highest residual (product wiring, not this crate alone):** When a future host ticket wires “Login with X” and a **wrong or phishing-shaped authorize URL** is passed in, the browser will open it. Mitigations **in this ticket**: https (or explicit loopback-http test flag), issuer origin/prefix check, no hard-coded client identity. Residual: a **compromised or mistaken caller** that supplies a malicious-but-same-issuer authorize path, or a lookalike issuer string, still opens a browser. **Defense:** host must pass issuer/authorization_endpoint from a trusted provider registry, not free text from chat.

2. **Refresh / code exchange against a wrong token_endpoint:** Fail-closed on non-2xx/parse errors, but a malicious endpoint that returns 200 with attacker-controlled tokens could store attacker tokens in DPAPI if the host persists them. **Mitigation path:** pin token_endpoint to the same issuer registry; TLS (https) in production; never log tokens.

3. **Device code social engineering:** `user_code` is meant to be shown; if UI also showed `device_code` or poll forms, that would expand takeover risk. This crate keeps device_code secret-bearing; host must not display it.

4. **Browser open OS integration:** uses `rundll32 url.dll,FileProtocolHandler` on Windows. URL is validated first; residual is OS handler misconfiguration (out of scope).

5. **Prepare path error coarsening:** `resolve_oauth_access_token` still maps transport failures to `OauthRefreshFailed` so existing slot prepare API stays fail-closed without leaking detail. Fine for security; operators get less granular diagnostics at prepare time (transport APIs themselves expose typed codes).

**Most likely step that could put a real user account at risk:**  
**Opening the system browser to an authorize URL that the host constructed from untrusted input (issuer / client_id / redirect mismatch).** This transport layer reduces that risk with scheme + issuer checks and no vendor identity hard-coding, but it cannot save a host that feeds it a hostile same-origin authorize URL. Total-control should keep provider metadata server-side / allowlisted when wiring UI login.

---

## Design sketch

```text
LoopbackListener::bind()  →  http://127.0.0.1:{port}/callback
build_authorize_url(...) + validate_authorize_url(url, issuer, policy)
open_authorize_with(opener, url, issuer, policy)
wait_for_code(expected_state, timeout) → AuthorizationCode(SecretBytes)
exchange_authorization_code(client, token_endpoint, client_id, code, redirect, verifier)
  → TokenResponse { access, refresh?, expires_in, ... }  // Debug redacted
  → DPAPI via existing CreateSlotAuth::Oauth (prior ticket)

Device:
request_device_authorization → show user_code only
poll_device_token_with_transport → pending | slow_down(+5s) | expired | tokens
```

---

**OAUTH-TRANSPORT-COMPLETE**
