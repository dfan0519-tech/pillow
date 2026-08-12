# OAuth browser open fix — execution report

**Date:** 2026-08-12  
**Worker:** grok2 (写码) · 总控: Claude  
**Completion signal:** `OAUTH-BROWSER-FIX-COMPLETE`

## 0. Scope

| Allowed | Touched |
| --- | --- |
| `crates/runtime-slots/**` | yes — `oauth_login.rs` orchestration; browser open hardening |
| `apps/desktop/src-tauri/**` | yes — `create_slot` oauth path, DTO fields, progress event |
| `apps/desktop/src/features/settings/**` | yes — error copy, copyable URL UI, GrokLogin hint |
| `apps/desktop/tests/**` | yes — browser-open failure + URL lift tests |
| This report | yes |

| Forbidden | Status |
| --- | --- |
| `crates/kernel-policy/**`, `crates/provider-adapters/**`, `docs/references/**` | not touched |
| Git write ops | none |
| Real user login / vendor credentials | none — tests use local mocks / injected failures only |

---

## 1. Exit-code table (actually run)

| Command | Log | EXIT |
| --- | --- | --- |
| `cargo test -p pillow-runtime-slots` | `docs/execution/_oauth-browser-fix-runtime-slots-full.log` | **0** (lib 45 + integration 21) |
| `node scripts/run-cargo.mjs test -p pillow-desktop --lib` | `docs/execution/_oauth-browser-fix-desktop-lib.log` | **0** (202 passed, 2 ignored) |
| `corepack pnpm typecheck` | `docs/execution/_oauth-browser-fix-typecheck.log` | **0** |
| `corepack pnpm --filter @pillow/desktop test` (focused: oauth + runtime-slots + settings-panel) | `docs/execution/_oauth-browser-fix-focused-test.log` | **0** (5 files / 83 tests) |
| `corepack pnpm --filter @pillow/desktop test` (full package) | `docs/execution/_oauth-browser-fix-desktop-test2.log` | **0** (53 files / 651 tests) |

> Gate invocation used `cmd > log 2>&1` (no `tail` pipe on the command itself).

---

## 2. T1 — breakpoint evidence (most important)

User report: choosing a config slot with **「登录认证」** does not open a webpage.

### 2.1 Which auth did the user mean?

| Badge / UI label | Auth kind | Opens browser? |
| --- | --- | --- |
| **Grok · 登录** | `GrokLogin` / `authMode: "login"` | **No — by design** |
| **OAuth · …** / create method「OAuth 登录」 | `oauth` | **Yes — intended** |

**Evidence that `GrokLogin` is not browser OAuth:**

- Create path only calls `gateway.createSlot({ authKind: "grok_login" })` with no browser/OAuth fields (`RuntimeSlotsSection.tsx`).
- Domain `CreateSlotAuth::GrokLogin` stores a profile reference only; `prepare` uses the machine Grok CLI login (`crates/runtime-slots/src/lib.rs`).
- Product copy already claimed CLI ownership (`loginGuidance.ts`: *「Grok 登录由 Grok CLI 管理」*).
- Host command `create_grok_login_slot` never calls `open_system_browser`.

**Conclusion for `Grok · 登录`:** not a rundll32 bug — **missing product affordance** if the user expects a browser popup. Fixed copy now states explicitly: no browser will open; run `grok login` in a terminal; use「OAuth 登录」for browser OAuth.

### 2.2 Ring-by-ring for OAuth (the path that *should* open a browser)

| # | Ring | Before this ticket | How confirmed |
| --- | --- | --- | --- |
| 1 | Renderer selects OAuth and invokes host | **Yes** — `TauriRuntimeSlotsGateway.createSlot` sends `authKind: "oauth"`, `providerId`, `oauthSupplement` | Source read of `runtimeSlotsGateway.ts` + UI form submit |
| 2 | Host command registered in `generate_handler!` | **`create_slot` yes**; catalog `list_provider_login_catalog` yes | `lib.rs` handler list + unit test pin |
| 3 | Host accepts `auth_kind: "oauth"` and runs login | **No** — `create_slot_impl` only matched `grok_login` / `api_key` / `local_cli`; oauth → `"Unknown auth_kind."` | Source of `runtime_slot_commands.rs` before change |
| 4 | Host reaches `open_system_browser` | **No** — transport helper existed but nothing in create path called it | Grep: only `oauth_transport` + tests; no command call site |
| 5 | `rundll32 url.dll,FileProtocolHandler` on real desktop | Implementable; spawn failure → `OauthBrowserOpenFailed` | Implementation in `oauth_transport.rs` (Windows); not silently swallowed after this fix |

**Primary defect:** OAuth **product wiring incomplete** (UI + transport + storage ready; host login orchestration missing). Silent/opaque failure: unknown `auth_kind` → generic「运行时槽位命令失败」or user confusion with GrokLogin.

---

## 3. T2 — OpenCode mechanism (read-only reference)

Tree: `_ref/opencode` (MIT, commit `d470434`). **Not copied wholesale.**

| OpenCode behavior | Pillow adoption |
| --- | --- |
| Always surface the login URL to the operator (`Prompt.log.info("Go to: " + login.url)`) then `open(url).catch(() => undefined)` (`packages/opencode/src/cli/cmd/account.ts`) | Progress event + copyable URL field; browser open failure is non-terminal in production (still wait for callback) |
| PKCE + loopback callback (`plugin/openai/codex.ts`) | Existing `oauth.rs` + `oauth_transport` loopback; new `oauth_login` composes them |
| Device code prints verification URI + user code, then opens browser | Same: progress carries `user_code` + verification URI; UI can show both |
| Uses npm `open` package | Windows host keeps `rundll32` (no new dependency); documented as equivalent intent |

### Suggested `THIRD_PARTY_NOTICES.md` entry (do **not** edit root file here)

```
OpenCode (anomalyco/opencode) — MIT
  Reference only for OAuth UX mechanics: always expose authorize URL, attempt
  system browser open without failing the whole flow on open() error, PKCE
  authorize URL + device-code verification surface.
  Commit d470434; no source files vendored into Pillow product crates.
```

---

## 4. What we fixed and why

### Domain (`crates/runtime-slots`)

- **New** `oauth_login.rs`: `perform_oauth_login` / `perform_oauth_login_production`
  - Resolves provider profile + public supplement → PKCE or device-code
  - Builds authorize URL, reports `OauthLoginProgressEvent` (public URL + `browser_opened` + optional `user_code`)
  - Attempts browser open; production continues waiting after open failure (OpenCode pattern)
  - On terminal failure, `OauthLoginFailure.authorize_url` preserves the public URL for UI fallback
- Unit tests prove **browser-open fail closed returns copyable authorize URL** (no tokens in Debug)

### Host (`apps/desktop/src-tauri`)

- `create_slot` accepts `provider_id` + `oauth_supplement` and **`auth_kind: "oauth"`**
- Runs `perform_oauth_login_production` with `SystemBrowser` + `WinHttpOauthTokenClient`
- Emits `runtime-slot-oauth-progress` (authorize URL / browser_opened / user_code)
- Maps `runtime_slot_oauth_browser_open_failed` (and loopback timeout with URL) so the public authorize URL is available to the renderer
- `RuntimeSlotDto` adds closed keys `accountLabel` + `accessExpiresAt` (null for non-OAuth)

### Renderer (`features/settings`)

- Error code `runtime_slot_oauth_browser_open_failed` → Chinese copy that is **not** silent
- `RuntimeSlotsGatewayError.authorizeUrl` lifted only when host message is a public http(s) URL
- OAuth form shows **copyable readonly authorize URL** + in-progress state; optional device user_code
- GrokLogin create hint clarifies **no browser** + CLI `grok login` + point users to OAuth for browser login
- Tauri progress listen is **dynamic import only inside a real Tauri shell** (avoids vitest hangs)

---

## 5. Browser-open failure → copyable URL (test evidence)

| Evidence | Result |
| --- | --- |
| `oauth_login::tests::browser_open_fail_closed_returns_copyable_authorize_url` | `code == runtime_slot_oauth_browser_open_failed`, URL starts with `https://as.example.test/oauth/authorize`, contains `client_id` + `code_challenge`, progress event `browser_opened: false` |
| `oauth_login::tests::browser_open_fail_still_reports_progress_then_times_out_with_url` | Production path: still times out with URL retained |
| `oauth_login::tests::device_browser_fail_closed_returns_verification_uri` | Device verification URI + user_code; secret device_code not in Debug |
| UI: `oauth-login-section` **browser open failed: shows readable copy + copyable authorize URL, no slot** | Error text = `OAUTH_BROWSER_OPEN_FAILED_MESSAGE`; readonly input value = public URL; slot list length 0 |
| Gateway: `normalizeRuntimeSlotsError lifts public authorize URL…` | URL lifted from host message; path-shaped host message → `authorizeUrl: null` |

No real vendor login and no user credentials were used.

---

## 6. Write-boundary audit + residual risks

### Touched paths

- `crates/runtime-slots/src/oauth_login.rs` (new)
- `crates/runtime-slots/src/lib.rs`, `oauth_transport.rs`
- `apps/desktop/src-tauri/src/runtime_slot_commands.rs`, `runtime_slot_commands_tests.rs`
- `apps/desktop/src/features/settings/{RuntimeSlotsSection,runtimeSlotsGateway,runtimeSlotsTypes,oauthLoginUi}.ts(x)`
- `apps/desktop/tests/{oauth-login-section,oauth-login-ui,runtime-slots-gateway}.*`
- This report

### Residual risks

1. **GrokLogin still does not open a browser.** Intentional. Users who only click「已登录的 Grok」must use CLI login; browser OAuth is the fourth auth method.
2. **Production browser-open failure continues waiting** (loopback timeout default 120s). UI should show the authorize URL via progress event; if the event is missed, timeout error still carries the URL on the host message for that code family.
3. **`rundll32` success ≠ browser visible** (OS association / policy). Mitigated by always exposing the URL, not by detecting GUI focus.
4. **Full interactive desktop E2E of a real authorize page** was not run (policy: do not complete real login). Manual smoke: select OAuth → 开始 OAuth 登录 → confirm browser *or* copyable URL appears.
5. **DTO dual deploy:** host now always serializes `accountLabel` / `accessExpiresAt`; renderer allowlist already had 19 keys.
6. **Device-code mid-flow UX** still does not stream polling status beyond user_code + URL (invoke remains blocking until complete).

---

## Completion

**`OAUTH-BROWSER-FIX-COMPLETE`**
