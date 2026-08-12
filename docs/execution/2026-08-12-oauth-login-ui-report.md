# OAuth 登录入口（渲染层）— 执行报告

**Status:** complete  
**Complete signal:** `OAUTH-LOGIN-UI-COMPLETE`  
**Date:** 2026-08-12  
**Worker slot:** grok2  
**Total control:** Claude  
**Write boundary:** `apps/desktop/src/**`, `apps/desktop/tests/**`, this report

## 0. Scope

Add the fourth runtime-slot auth entry **OAuth 登录** in the desktop renderer:

- Data-driven row on `SLOT_AUTH_METHODS` (no new auth branch tree).
- Provider picker from the login catalog; `api_key_only` excluded; `Unknown` /
  `UserSupplied` fields require explicit user input (never auto-guessed).
- Async login UX with in-progress state; timeout / denied / state-mismatch
  each get distinct copy; failure never paints a logged-in slot.
- Logged-in row shows non-sensitive `accountLabel` + `accessExpiresAt` only.
- Renderer never opens vendor sockets; host owns the login transport.
- Tests use an injectable mock gateway only — no real login, no real accounts.

### Hard safety

1. No real OAuth, no vendor endpoints, no user credentials.
2. Token / refresh token / authorization code never enter DOM, logs, or
   response views.
3. OAuth slots remain governed (`allowsController: true`); local CLI still
   cannot be controller (existing one-slot-flow tests not weakened).
4. Host DTO widenings update the closed key allowlist in the same change,
   with accept + reject tests.

---

## 1. Exit code table (actually run)

Capture style (cmd, no pipe to `tail` for the exit code):

```text
cmd /c "corepack pnpm typecheck > docs\execution\_oauth-login-ui-typecheck.log 2>&1 & echo EXIT=%ERRORLEVEL%"
cmd /c "corepack pnpm --filter @pillow/desktop test > docs\execution\_oauth-login-ui-test.log 2>&1 & call echo EXIT=%ERRORLEVEL%"
```

| Command | EXIT | Notes |
| --- | ---: | --- |
| `corepack pnpm typecheck` | **0** | Workspace recursive `tsc --noEmit` green |
| `corepack pnpm --filter @pillow/desktop test` | **0** | **53** files, **641** tests passed (0 failed); ~37s |

OAuth-focused suites (from the same full run):

| File | Result |
| --- | --- |
| `tests/oauth-login-ui.test.ts` | 10 passed |
| `tests/oauth-login-section.test.tsx` | 13 passed |
| `tests/runtime-slots-gateway.test.ts` | 25 passed (includes oauth DTO + mock create) |
| Existing `tests/runtime-slots-one-slot-flow.test.tsx` | still green (CLI cannot be controller) |

Temporary gate logs under `docs/execution/_oauth-login-ui-*.log` may be deleted after review; they are not product artifacts.

---

## 2. T2 — Unknown fields require user input (+/−)

Pure helpers: `oauthFieldsRequiringUserInput` / `oauthFieldRequiresUserInput` /
`oauthCapableProviders` in `oauthLoginUi.ts`.

| Case | Direction | Evidence |
| --- | --- | --- |
| `api_key_only` filtered out of OAuth list | +/− | Unit: `oauthCapableProviders` keeps openai/xai/custom-oidc, drops anthropic. UI: `runtime-slots-oauth-provider-ids` never contains `anthropic`. |
| Unknown `client_id` (openai / xai / github-copilot) | + | Requires `clientId` field; reason = `OAUTH_FIELD_CLIENT_ID_UNKNOWN_REASON`; input value starts empty (no guess). Tests: `oauth-login-ui.test.ts`, `oauth-login-section.test.tsx`. |
| UserSupplied fields (custom-oidc) | + | Requires clientId, issuer, authorizeEndpoint, tokenEndpoint, scopes, accountLabel — each with a reason note. |
| Known fields (openai issuer / authorize / token / scopes) | − | Must **not** require user input; UI does not render those field testids for openai. |
| NotApplicable device endpoint on PKCE | − | `deviceAuthorizationEndpoint` not required for openai. |

Catalog rows never auto-fill `knownClientId` when `clientId === "unknown"`;
parser rejects that combination (`parseProviderLoginCatalogEntry`).

---

## 3. T3 — Three failure modes (copy + tests)

| Mode | User-facing copy constant | Host error codes mapped | Tests |
| --- | --- | --- | --- |
| Timeout | `OAUTH_LOGIN_TIMEOUT_MESSAGE` — 「登录超时：在限定时间内未完成浏览器授权。请重试。」 | `runtime_slot_oauth_login_timeout`, `…_loopback_timeout`, `…_http_timeout`, `…_device_expired` | pure map + UI mock inject; snapshot length 0; no list |
| Denied | `OAUTH_LOGIN_DENIED_MESSAGE` — 「登录被拒绝：您在授权页面取消了授权，或提供方拒绝了本次请求。」 | `…_login_denied`, `…_loopback_denied`, `…_device_denied` | pure map + UI mock inject; no slot |
| State mismatch | `OAUTH_LOGIN_STATE_MISMATCH_MESSAGE` — 「登录失败：授权回调校验未通过（state 不匹配）…」 | `…_login_state_mismatch`, `…_loopback_state_mismatch` | pure map + UI mock inject; no slot |

In-progress:

- Constant `OAUTH_LOGIN_IN_PROGRESS` shown under `data-testid="runtime-slots-oauth-in-progress"`.
- Save button label becomes 「登录中…」; form stays open until host returns.
- Deferred mock: list remains empty while pending; on failure still empty (no optimistic paint).

Three copy strings are asserted distinct in unit tests.

---

## 4. Token / code never in DOM (test evidence)

| Evidence | Where |
| --- | --- |
| Success create UI asserts sentinels absent from `textContent`, `innerHTML`, and input values | `oauth-login-section.test.tsx` — `assertMaterialAbsentFromDom(TOKEN_SENTINEL, REFRESH_SENTINEL, CODE_SENTINEL, …)` |
| Slot JSON from mock has no `access_token` / `refresh_token` | same file + `MockRuntimeSlotsGateway` oauth create test |
| `assertNoSecretPayloadFields` rejects `access_token`, `refreshToken`, `authorization_code`, `token`, … | `runtime-slots-gateway.test.ts` |
| Closed allowlist rejects smuggled `accessToken` / `refresh_token` on slot DTO | same |
| Mock stores access/refresh only under opaque secret map; view has `secretRef: null` for OAuth | `runtimeSlotsGateway.ts` mock `createSlot` oauth branch |
| List row shows only `accountLabel` + formatted `accessExpiresAt` | `RuntimeSlotsSection` + list fixture test |

No `console.log` of host errors was added (existing secret-safe pattern retained).

---

## 5. Host command contract

Renderer expects these host commands / DTO shapes. If the host worker has not
landed them yet, this is the closed contract to implement (no secrets on wire).

### 5.1 `list_provider_login_catalog`

- **Args:** `{}` (no project needed for public catalog).
- **Returns:** `ProviderLoginCatalogEntryDto[]` (max 64).

Closed keys (`PROVIDER_LOGIN_CATALOG_DTO_KEYS`):

| Key | Type | Notes |
| --- | --- | --- |
| `providerId` | string | e.g. `openai`, `xai`, `custom-oidc` |
| `displayName` | string | UI label |
| `loginMethod` | `"pkce" \| "device_code" \| "api_key_only"` | UI filters out `api_key_only` |
| `issuer` | `"known" \| "user_supplied" \| "unknown" \| "not_applicable"` | field state |
| `authorizeEndpoint` | same enum | |
| `deviceAuthorizationEndpoint` | same enum | |
| `tokenEndpoint` | same enum | |
| `scopes` | same enum | |
| `clientId` | same enum | |
| `knownIssuer` | string \| null | only when Known |
| `knownAuthorizeEndpoint` | string \| null | |
| `knownDeviceAuthorizationEndpoint` | string \| null | |
| `knownTokenEndpoint` | string \| null | |
| `knownScopes` | string[] \| null | |
| `knownClientId` | string \| null | **must be null** when `clientId` is `unknown` |
| `accountIdentifier` | `"token_claims" \| "user_supplied" \| "unknown" \| "not_applicable"` | |

Never include tokens, codes, client secrets, or guessed client ids.

### 5.2 `create_slot` with `authKind: "oauth"`

(Or a dedicated `create_oauth_slot` that returns the same `RuntimeSlotView` —
renderer currently uses unified `create_slot`.)

**Args (camelCase):**

| Key | Required | Notes |
| --- | --- | --- |
| `projectId` | yes | |
| `displayName` | yes | |
| `authKind` | yes | `"oauth"` |
| `role` / `roleCustomName` | role rules same as other auth | OAuth may be controller |
| `baseUrl` | yes | OpenAI-compatible endpoint for grok shell |
| `modelId` | yes | |
| `providerId` | yes | catalog id |
| `oauthSupplement` | when Unknown/UserSupplied | public only: `clientId`, `issuer`, `authorizeEndpoint`, `deviceAuthorizationEndpoint`, `tokenEndpoint`, `scopes[]`, `accountLabel` |

Host performs browser / device-code flow **inside the invoke** (or until terminal
failure). Must not return success until tokens are stored host-side.

**Error codes** (mapped to the three UI messages):

| Code family | UI |
| --- | --- |
| `runtime_slot_oauth_login_timeout` / `…_loopback_timeout` / `…_http_timeout` / `…_device_expired` | timeout copy |
| `runtime_slot_oauth_login_denied` / `…_loopback_denied` / `…_device_denied` | denied copy |
| `runtime_slot_oauth_login_state_mismatch` / `…_loopback_state_mismatch` | state mismatch copy |

### 5.3 `RuntimeSlotDto` widenings (closed allowlist now 19 keys)

New optional-on-wire fields (parsed null if absent):

| Key | Type | Notes |
| --- | --- | --- |
| `accountLabel` | string \| null | Non-sensitive account id; never token |
| `accessExpiresAt` | string \| null | RFC3339 access-token expiry |

Also: `authMode` may be `"oauth"` (alongside `login` / `api_key`).

For OAuth rows host should set:

- `kind: "governed_grok"`, `governance: "full"`, `authMode: "oauth"`
- `secretRef: null` (do not surface DPAPI refs for access/refresh)
- `secretConfigured: true` when tokens are stored
- `accountLabel` / `accessExpiresAt` as available
- **Never** `accessToken`, `refreshToken`, `authorizationCode`, etc.

Renderer allowlist pin: `RUNTIME_SLOT_DTO_KEYS.length === 19` (was 17).

---

## 6. Write-boundary audit + residual risks

### Touched (in boundary)

| Path | Change |
| --- | --- |
| `apps/desktop/src/features/settings/runtimeSlotsTypes.ts` | `oauth` auth kind + `SLOT_AUTH_METHODS` row; OAuth catalog types; error codes; guidance copy; slot fields |
| `apps/desktop/src/features/settings/oauthLoginUi.ts` | **new** pure helpers + mock catalog |
| `apps/desktop/src/features/settings/runtimeSlotsGateway.ts` | allowlists, parsers, secret key guards, Tauri + Mock catalog/create |
| `apps/desktop/src/features/settings/RuntimeSlotsSection.tsx` | OAuth form, in-progress, failures, list display |
| `apps/desktop/tests/oauth-login-ui.test.ts` | **new** pure tests |
| `apps/desktop/tests/oauth-login-section.test.tsx` | **new** UI tests |
| `apps/desktop/tests/runtime-slots-gateway.test.ts` | DTO accept/reject + mock oauth |
| Fixture updates in existing desktop tests | `accountLabel` / `accessExpiresAt` on `RuntimeSlotView` |
| `docs/execution/2026-08-12-oauth-login-ui-report.md` | this report |

### Not touched (as required)

- `crates/**`
- `apps/desktop/src-tauri/**`
- `docs/references/**`
- Git write operations
- Real OAuth / vendor endpoints / user accounts
- Full gate / E2E (total-control exclusive)

### Residual risks

1. **Host not wired yet:** Tauri will reject `list_provider_login_catalog` /
   `create_slot` oauth until the host worker lands matching commands and
   `RuntimeSlotDto` fields. Renderer fails closed with mapped errors.
2. **DTO dual deploy:** Host must ship `accountLabel` / `accessExpiresAt` /
   `authMode: "oauth"` together with this allowlist; unknown keys still throw.
3. **Device-code UX:** Form does not yet show `user_code` / verification URI
   mid-flow (host may surface those later via events); current UX is
   “in progress until invoke returns”.
4. **Consumer subscriptions:** Still out of product scope (prior oauth-slots
   report); UI does not claim ChatGPT Plus / Claude Pro work.

---

## Completion

**`OAUTH-LOGIN-UI-COMPLETE`**
