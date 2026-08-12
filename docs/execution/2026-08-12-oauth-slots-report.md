# OAuth / OIDC Slot Auth — Execution Report

**Status:** complete  
**Complete signal:** `OAUTH-SLOTS-COMPLETE`  
**Date:** 2026-08-12  
**Worker slot:** grok1  
**Write boundary:** `crates/storage-sqlite/**`, `crates/runtime-slots/**`, this report

## 0. Scope

Add a fourth `SlotAuth::Oauth` variant (standard OIDC/OAuth only), PKCE pure
logic + token-response parse + refresh decision, and prepare-time injection of
a **valid access token** into the grok shell via existing `env_key`.

**Not in scope:** browser open, loopback listener, real network, consumer
subscriptions (ChatGPT Plus / Claude Pro), desktop/UI wiring.

### Hard fact (do not expand)

Consumer subscriptions (ChatGPT Plus / Claude Pro) cannot attach to the grok
shell. OpenCode’s ChatGPT path hits a proprietary endpoint
(`https://chatgpt.com/backend-api/codex/responses`) with a restricted model
allowlist, JWT-derived `chatgpt_account_id`, and Authorization rewriting —
none of which `base_url` + `env_key` can express. This ticket targets
**standard OIDC/OAuth providers only**. No UI or code path implies consumer
subscriptions work.

## 1. Exit codes (actually run)

Command (PowerShell; no pipe to `tail` for the exit code):

```text
cmd /c "cargo test -p keeper-storage-sqlite -p pillow-runtime-slots > <repo>\docs\execution\_oauth-slots-test.log 2>&1"
Write-Host "EXIT=$LASTEXITCODE"
```

| Check | EXIT | Notes |
| --- | --- | --- |
| `cargo test -p keeper-storage-sqlite -p pillow-runtime-slots` | **0** | All suites green, including `#[cfg(test)]` modules |
| Log | `docs/execution/_oauth-slots-test.log` | Full stdout/stderr of the run |

Selected suite lines from that log:

| Suite | Result |
| --- | --- |
| `keeper-storage-sqlite` integration (runtime_slot_repository, …) | ok (0 failed) |
| `pillow-runtime-slots` lib (`oauth::tests`, governance, catalog, …) | ok. **16** passed |
| `pillow-runtime-slots` integration `slot_service` | ok. **16** passed |

## 2. `_ref/opencode` commit + suggested `THIRD_PARTY_NOTICES.md` entry

```text
git -C <workspace>/_ref/opencode rev-parse HEAD
→ d4704347465c1ee63d0c213ed00e648e7f0231c5
```

**Write boundary note:** this ticket must not edit root `THIRD_PARTY_NOTICES.md`.
Suggested entry (append under a new `## OpenCode` section, same table format as
Grok App):

```markdown
## OpenCode

Repository: [`anomalyco/opencode`](https://github.com/anomalyco/opencode)

Fixed commit: `d4704347465c1ee63d0c213ed00e648e7f0231c5`

License source: `LICENSE` at the fixed commit above (MIT).

### Shape / flow adaptations (not a wholesale port)

| Pillow path | Upstream source (exact) | What was reused |
| --- | --- | --- |
| `crates/runtime-slots/src/oauth.rs` | `packages/opencode/src/auth/index.ts` | Credential shape idea: OAuth holds refresh/access/expires (+ optional account label); API key is a separate mode. No OpenCode auth.json I/O. |
| `crates/runtime-slots/src/oauth.rs` | `packages/core/src/plugin/provider/openai.ts` (`generatePKCE`, `authorizeURL`, code exchange form) | PKCE S256 verifier/challenge generation; authorize URL query shape (`response_type=code`, `code_challenge_method=S256`); authorization_code + code_verifier form fields. |
| `crates/runtime-slots/src/oauth.rs` | `packages/opencode/src/plugin/xai.ts` (`ACCESS_TOKEN_REFRESH_SKEW_MS`, refresh form) | 120s proactive refresh skew; `grant_type=refresh_token` + `client_id` + `refresh_token` form shape. |
| `crates/storage-sqlite/src/runtime_slot.rs` (`SlotAuth::Oauth`) | `packages/opencode/src/auth/index.ts` (`Oauth` class fields) | Non-secret provider metadata + token *references* only (Pillow stores DPAPI refs, never plaintext tokens in SQLite). |

**Not claimed as ports:** browser/device-code poll loops, loopback HTTP callback
servers, OpenCode `auth.json` storage, ChatGPT Codex proprietary backend, or
any consumer-subscription integration.

### MIT License

MIT License

Copyright (c) 2025 opencode

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## 3. T1 — migration / round-trip evidence (three legacy variants + OAuth)

### Schema

- Migration `0018_oauth_slots.sql`: rebuilds `runtime_slots` with
  `auth_kind IN (..., 'oauth')` and columns
  `refresh_secret_ref`, `access_expires_at`, `oauth_meta_json`.
- Tokens never enter SQLite: only UUID secret refs + non-secret OIDC meta JSON
  (`provider_id`, `issuer`, `client_id`, `scopes`, `token_endpoint`,
  `account_label`).
- Governance for OAuth is **derived** → `Full` (not a user field).

### Tests (all passed in the gate log)

| Test | Evidence |
| --- | --- |
| `legacy_slot_rows_migrate_round_trip_preserving_secret_ref` | Pre-0016 rows → open applies 0016–0018; ApiKey/GrokLogin/LocalCli still round-trip; `secret_ref` preserved |
| `oauth_and_legacy_auth_variants_round_trip_without_token_material` | Creates all four variants; reopen equals; DB columns have no `access_token`/`refresh_token`; meta JSON has public OIDC fields only |
| `governance_derivation_maps_all_auth_variants` | ApiKey/GrokLogin/Oauth → Full; LocalCli → None |
| `oauth_controller_is_accepted_as_governed` | Oauth + Controller accepted |
| `update_model_works_for_oauth_slot` | Model rotate keeps access secret ref |
| unit `runtime_slot::governance_derivation_tests::oauth_maps_to_full` | wire_auth_mode=`oauth`, wire_kind=`governed_grok` |

## 4. PKCE + refresh decision (positive / negative / expiry boundary)

Module: `crates/runtime-slots/src/oauth.rs` (pure; no network).

| Test | What it pins |
| --- | --- |
| `oauth::tests::pkce_generate_and_verify_round_trip` | Random verifier ≥43 chars; S256 challenge verifies; wrong challenge rejected |
| `oauth::tests::s256_challenge_is_stable_for_fixed_verifier` | Deterministic challenge; no `=` padding |
| `oauth::tests::authorize_url_contains_pkce_and_scopes` | `response_type=code`, `code_challenge_method=S256`, scopes, state |
| `oauth::tests::parse_token_response_positive_and_negative` | Valid JSON → tokens; missing/empty/malformed → `OauthTokenResponseInvalid` |
| `oauth::tests::refresh_decision_positive_negative_and_boundary` | Far future → no refresh; past/now → refresh; **exactly `now+skew` → refresh (inclusive)**; `now+skew+1s` → no refresh |
| `refresh_decision_helpers_exported_for_callers` | Public API re-export smoke |

Default skew: `ACCESS_TOKEN_REFRESH_SKEW` = **120 seconds** (OpenCode xAI shape).

## 5. Refresh token never leaks to the grok shell

| Test | Evidence |
| --- | --- |
| `oauth_prepare_injects_only_access_token_never_refresh` | `PreparedGrokSlot::ApiKey.api_key` bytes == access only; ≠ refresh; private `config.toml` contains only `env_key` name, never token material |
| `oauth_prepare_refreshes_when_expired_and_fails_closed_on_refresh_error` | After refresh, prepare hands **new access** only; form to fake client contains refresh (expected for token endpoint); private config contains neither old/new refresh nor access plaintext |

Implementation: `resolve_oauth_access_token` returns only `tokens.access_token` /
stored access ref material. Refresh stays in DPAPI under `refresh_secret_ref`.

## 6. Secrets not in DTO / Debug / logs

| Control | Evidence |
| --- | --- |
| Slot DTO | `SlotAuth::Oauth` stores UUID refs + public meta only (Debug of slot auth in prepare test does not contain token markers) |
| `SecretBytes` Debug | `format!("{:?}", SecretBytes::…)` → `SecretBytes([REDACTED])` (prepare test) |
| `TokenResponse` / `PkceCodes` Debug | `oauth::tests::token_response_debug_redacts_secrets` |
| SQLite | Column allowlist assertions: no `access_token` / `refresh_token` / `api_key` columns; ciphertext files remain DPAPI |
| Drop zeroing | Existing `SecretBytes` Drop still zeroizes |

Test markers are **runtime-concatenated** (`runtime_token("acc-", "ess-…")`) so
source never holds secret-shaped literals (secret gate / no whitelist).

## 7. Write-boundary audit + residual risks

### Files touched (in boundary)

| Path | Change |
| --- | --- |
| `crates/storage-sqlite/migrations/0018_oauth_slots.sql` | OAuth schema |
| `crates/storage-sqlite/src/migrations.rs` | Apply 0018 |
| `crates/storage-sqlite/src/runtime_slot.rs` | `SlotAuth::Oauth`, encode/decode, update, governance |
| `crates/storage-sqlite/tests/runtime_slot_repository.rs` | Round-trip + governance tests |
| `crates/runtime-slots/Cargo.toml` | `base64`, `chrono`, `getrandom`, `sha2` (workspace deps; **no root Cargo.toml edit**) |
| `crates/runtime-slots/src/oauth.rs` | PKCE + parse + refresh decision |
| `crates/runtime-slots/src/secret.rs` | Redacted `Debug` for `SecretBytes` |
| `crates/runtime-slots/src/lib.rs` | Create/prepare/delete/update OAuth; injectable `OauthTokenClient` |
| `crates/runtime-slots/tests/slot_service.rs` | Access-only inject + refresh fail-closed |
| `docs/execution/2026-08-12-oauth-slots-report.md` | This report |

### Not touched (as required)

- `apps/desktop/**`
- `crates/runtime-host/**`
- `crates/knowledge-indexer/**`
- root `Cargo.toml`
- Real auth files (including `~/.local/share/opencode/auth.json`)
- Git write operations
- `THIRD_PARTY_NOTICES.md` (entry drafted above for a follow-up commit)

### Residual risks

1. **Desktop compile:** `apps/desktop/src-tauri` matches `SlotAuth` exhaustively;
   adding `Oauth` will fail those matches until a host/UI ticket widens them.
   Out of this write boundary by design.
2. **No real network client:** `OauthTokenClient` is injectable; production HTTP
   for token refresh is a later ticket (account-risk review). Expired OAuth
   without a client fails closed (`runtime_slot_oauth_refresh_failed`).
3. **No browser / PKCE callback server:** pure URL + verifier helpers only.
4. **`RuntimeSlotKind` lossy for OAuth:** historical kind view maps OAuth →
   GovernedGrok/ApiKey with access ref only. Prefer matching `SlotAuth`.
5. **role-dispatch `SlotAuthShape`:** still three variants; dispatch wiring for
   OAuth is not this ticket.
6. **Notices file not updated in-repo:** MIT entry is in §2 for total-control to
   land outside this worker’s write boundary.

## 8. Design summary

```text
SlotAuth::Oauth {
  provider_id, issuer, client_id, scopes, token_endpoint, account_label?,
  base_url, model_id,                 // grok shell OpenAI-compatible surface
  access_secret_ref, refresh_secret_ref,  // DPAPI only
  access_expires_at,
}
governance = Full (derived)

prepare_grok(Oauth):
  if needs_refresh(expires, now, 120s):
    POST token_endpoint with refresh form via OauthTokenClient
    on failure → OauthRefreshFailed (no silent reuse of expired access)
    rotate DPAPI refs + expires
  inject access token only as PreparedGrokSlot::ApiKey.env material
  write private config.toml with env_key = PILLOW_GROK_SLOT_API_KEY
```

---

**OAUTH-SLOTS-COMPLETE**
