# First-run → “can chat” (renderer-only) — execution report

**Worker:** grok1  
**Date:** 2026-08-11  
**Scope:** `apps/desktop/src/**`, `apps/desktop/tests/**`, this report only  
**Signal:** `FIRSTRUN-SLOTS-COMPLETE`

## Status

Complete. T1–T3 implemented under the write boundary; typecheck and desktop unit tests green.

## Tasks

| ID | Description | Status |
|----|-------------|--------|
| T1 | First-run onboarding: slot setup step after open project | done |
| T2 | Runtime-unavailable: actionable tip (no slots vs slots not started) | done |
| T3 | Inactive governed slots: Start affordance obvious | done |

## 1. Exit codes

Commands run **without** piping to `tail` (exit codes preserved).

| Command | Exit code | Notes |
|---------|-----------|-------|
| `corepack pnpm typecheck` | **0** | `@pillow/desktop` `tsc --noEmit` |
| `corepack pnpm --filter @pillow/desktop test` | **0** | 39 files / **511** tests passed |

## 2. T2 test evidence

Suite: `apps/desktop/tests/runtime-unavailable-guidance.test.tsx`  
Helpers: `apps/desktop/src/features/settings/runtimeUnavailableGuidance.ts`

### 2.1 No slots

| Assertion | Evidence |
|-----------|----------|
| Send fails with host-shaped `runtime_unavailable` | `MockConversationGateway.failNextSend(RUNTIME_UNAVAILABLE_RAW)` |
| Slot list empty | `MockRuntimeSlotsGateway([])` |
| Banner kind | `data-runtime-slot-guidance="no_slots"` on `runtime-slot-guidance-banner` |
| Body is fixed actionable copy (not bare code) | `composer-send-error` and `runtime-slot-guidance-body` equal `RUNTIME_SLOT_GUIDANCE_NO_SLOTS` |
| Copy says create + start | Matches `/还没有可启动的代理槽位/` and `/新增/` |
| Settings CTA works | Click `runtime-slot-guidance-open-settings` → `settings-panel` + `runtime-slots-section` visible |
| No fake success | No `composer-refresh-warning`; send rejects (draft restore path) |

### 2.2 Slots exist but not started

| Assertion | Evidence |
|-----------|----------|
| Governed slot present, runtime still unavailable | `MockRuntimeSlotsGateway([governed login slot])` + same failNextSend |
| Banner kind | `data-runtime-slot-guidance="not_started"` |
| Distinct body | Equals `RUNTIME_SLOT_GUIDANCE_NOT_STARTED` (`/已有代理槽位/`); **not** the no-slots string |
| Settings CTA → Start visible | Opens settings; `runtime-slot-needs-start-slot-g1` + probe button text **「启动」** |

### 2.3 External-only slots

Only `external_cli` → classified as **`no_slots`** (cannot chat / cannot start as agent). Asserted in the same suite.

### 2.4 Negative controls (must not false-positive)

| Case | Assertion |
|------|-----------|
| Ordinary failure (`network down`) | `composer-send-error` = raw message; **no** `runtime-slot-guidance-banner` |
| Successful send | No `composer-send-error`; **no** banner |

Pure helpers also pin detection (phrase/code match) and classification (empty / external-only / governed).

## 3. Onboarding step copy (verbatim)

**Title**

```text
欢迎使用 Pillow
```

**Project step** (`FIRSTRUN_PROJECT_BODY`)

```text
Pillow 以项目为单位工作。打开一个本地文件夹即可开始协作与对话。
```

**Slot step** (`FIRSTRUN_SLOT_BODY` — after open project, before chat)

```text
能对话之前，还需在「设置 → 运行时槽位」配置并启动一个代理槽位。两种方式：① 使用已登录的 Grok（新建「登录 profile」槽位，无需填写 API key，然后点「启动」）；② 使用自己的 API key（新建「API key」槽位，填写端点与模型，保存后点「启动」）。外部 CLI 仅供登记与账号状态查询，不能用来对话。
```

**CTA labels:** 「打开项目」 / 「打开设置」 / 「跳过」 / 「关闭引导」

## 4. Write-boundary audit

| Path area | Touched? |
|-----------|----------|
| `apps/desktop/src/**` | **yes** |
| `apps/desktop/tests/**` | **yes** |
| `docs/execution/2026-08-11-firstrun-slots-report.md` | **yes** (this report) |
| `crates/**` | **no** |
| `apps/desktop/src-tauri/**` | **no** |
| `scripts/**` | **no** |
| git write ops | **no** |

### Source / test files changed

| File | Role |
|------|------|
| `src/features/onboarding/onboardingCopy.ts` | Slot-step copy |
| `src/features/onboarding/FirstRunOnboarding.tsx` | Renders `firstrun-slot-body` |
| `src/features/settings/runtimeUnavailableGuidance.ts` | **new** — classify + fixed copy |
| `src/App.tsx` | On send failure: list slots → banner + composer tip + settings CTA |
| `src/features/settings/RuntimeSlotsSection.tsx` | Primary **「启动」**; session **未启动 / 已启动** badges |
| `src/styles/runtime-slots.css` | Start button + badges |
| `src/styles/settings.css` | `runtime-slot-guidance-banner` |
| `tests/firstrun-onboarding.test.tsx` | Slot-step assertions |
| `tests/runtime-unavailable-guidance.test.tsx` | **new** — T2 pos/neg |
| `tests/runtime-slots-ui.test.tsx` | T3 Start affordance |

### Hard rules held

1. **`external_cli`**: no Start button; ungoverned copy unchanged; onboarding says cannot chat; classification excludes it from “chat-capable”.
2. **Secrets**: never displayed; onboarding does not echo keys; slot forms unchanged (`type="password"`, list only 已配置).
3. **No fake success**: runtime unavailable → error + guidance; send still throws; no silent mock turn.

## 5. Residual risk — other entry points that can look chat-ready without runtime

| Entry | Risk | Mitigation / gap |
|-------|------|------------------|
| Composer always enabled when a project is open | User can type and send before any slot is started | **Mitigated on failure** by T2 banner + tip; **not** blocked preemptively (honest: no host “active slot” DTO on list) |
| Composer context chip `运行时：本地` / `模拟` | Suggests a runtime is present even when none is active | Residual; chip is static product label, not live slot status |
| `get_runtime_startup` “ready” while process has a default service | Host may still open a boot-time service independent of slots (renderer cannot change host) | Residual host/process fact; slot start remains the product path for governed agents |
| “新建对话” succeeds in mock unit tests without slots | Tests use `MockConversationGateway` which always has a runtime | Production Tauri path fails closed with `runtime_unavailable` → T2 guidance |
| Dismiss first-run onboarding | Stranger can skip the slot tip | Expected; T2 still catches first failed send |
| Session-local “已启动” badge only | After app restart, list DTO has no `active` flag → all governed slots show **未启动** until Start again | Residual without host field; re-Start is the correct recovery (process-local activation) |
| List slots fails during T2 classify | Defaults to **no_slots** copy | Still sends user to Settings; slightly wrong if slots existed but list failed |

## Implementation notes

- T2 classifies only `kind === "governed_grok" && governance === "full"` as chat-capable.
- T3: governed rows use primary **「启动」** (after success: **「重新启动」** + **已启动** badge). External CLI remains **查询账号状态** only.
- `FIRSTRUN_GROK_BODY` alias retained pointing at `FIRSTRUN_SLOT_BODY` for any stale imports.

---

**FIRSTRUN-SLOTS-COMPLETE**
