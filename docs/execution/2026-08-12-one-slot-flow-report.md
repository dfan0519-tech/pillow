# One-slot flow — 新建槽位：命名 → 选认证 → 分配角色

Date: 2026-08-12  
Worker: grok1（写码）  
总控: Claude  

## Status

**DONE** — `ONE-SLOT-FLOW-COMPLETE`

## Scope (allowlist)

Touched:

| Path | Change |
| --- | --- |
| `apps/desktop/src/features/settings/RuntimeSlotsSection.tsx` | Unified create UI; role reassign; T3 guidance |
| `apps/desktop/src/features/settings/runtimeSlotsTypes.ts` | Auth catalog, role options, guidance copy, `UpdateSlotInput` |
| `apps/desktop/src/features/settings/runtimeSlotsGateway.ts` | `updateSlot` (Tauri + Mock); `lastCreateSlotArgs` |
| `apps/desktop/tests/runtime-slots-ui.test.tsx` | Align with unified create entry |
| `apps/desktop/tests/runtime-slots-one-slot-flow.test.tsx` | New: T1–T4, T3 dual, CLI ±, dual-controller |
| `docs/execution/2026-08-12-one-slot-flow-report.md` | This report |

Out of scope (untouched): `crates/**`, `apps/desktop/src-tauri/**`, `scripts/**`. No git writes.

## Tasks

| ID | Goal | Status |
| --- | --- | --- |
| T1 | Single create entry: name → auth → role via `createSlot` | **done** |
| T2 | Existing slots can change role | **done** |
| T3 | Guidance: no slots vs slots-but-no-controller | **done** |
| T4 | UI retires three legacy create call sites (gateway methods kept) | **done** |

## Exit codes (real runs)

| Command | Exit | Evidence |
| --- | --- | --- |
| `corepack pnpm typecheck` | **0** | Root recursive typecheck; `@pillow/desktop` `tsc --noEmit` clean. Re-run after final UI fix also **0**. |
| `corepack pnpm --filter @pillow/desktop test` | **0** | Vitest: **41 files / 529 tests passed** (includes `runtime-slots-one-slot-flow.test.tsx`). |

Intermediate RED before secret-clear-on-submit fix: first desktop test run exited **1** (2 failures: failure-path `apiKey` still in controlled input DOM). Fixed by clearing draft before `await createSlot` / `updateGrokApiKeySlot`; re-run **0**.

## T3 guidance evidence

File: `apps/desktop/tests/runtime-slots-one-slot-flow.test.tsx`  
Describe: `One-slot flow — T3 guidance (no slots vs no controller)`

| Case | Positive | Negative |
| --- | --- | --- |
| **Zero slots** | `runtime-slots-guidance-no-slots` visible with `GUIDANCE_NO_SLOTS_HEADING` / `GUIDANCE_NO_SLOTS_BODY` (“还没有槽位…请先新建…”) | `runtime-slots-guidance-no-controller` **absent**; `GUIDANCE_NO_CONTROLLER_HEADING` **absent** |
| **Slots, no controller** | `runtime-slots-guidance-no-controller` with heading/body (“尚未指定总控…当前已有槽位…”) | `runtime-slots-guidance-no-slots` **absent**; empty-state heading **absent** |
| **Controller present** | (optional) librarian soft tip if no librarian | Both missing-controller banners **absent** |

Constants (types module):

- `GUIDANCE_NO_SLOTS_HEADING` = `还没有槽位`
- `GUIDANCE_NO_SLOTS_BODY` = `请先新建一个槽位。没有总控时无法在 Pillow 里对话；建议将第一个受治理槽位（API key 或已登录的 Grok）设为总控。也可稍后指定信息管理员（不强制）。`
- `GUIDANCE_NO_CONTROLLER_HEADING` = `尚未指定总控`
- `GUIDANCE_NO_CONTROLLER_BODY` = `当前已有槽位，但没有总控。没有总控就无法在 Pillow 里对话。请将某个受治理槽位（API key 或已登录的 Grok）的角色改为总控，或新建一个总控槽位。建议再设一位信息管理员（不强制）。`
- `GUIDANCE_LIBRARIAN_SUGGESTION` = `建议设置一位信息管理员，便于知识与资料侧协助（不强制）。`

## CLI vs controller (+/−) evidence

Describe: `One-slot flow — CLI cannot be controller (+/−)`

| Assertion | Evidence |
| --- | --- |
| **− CLI → 总控不可选** | Select `runtime-slots-auth-local_cli` → `runtime-slots-role-controller` **disabled**; `runtime-slots-controller-cli-disabled` shows `CONTROLLER_REQUIRES_GOVERNED_AUTH_HINT`; librarian/custom **not** disabled |
| **+ API key → 总控可选** | Select `runtime-slots-auth-api_key` → controller **enabled**; CLI-disabled hint **absent**; can press controller (`aria-pressed=true`) |
| Switch API→CLI | Controller auto-downgrades (`aria-pressed=false`) and disables; hint reappears |
| Existing CLI row | Role form: controller option disabled + per-row hint; librarian still enabled |

Hint copy: `总控必须使用受治理认证（API key 或已登录的 Grok）。本机 CLI 不受治理，不能担任总控。`

## Create-flow copy (verbatim)

**Section head**

- Title: `运行时槽位`
- Blurb: `所有槽位平等：新建时填写名称、选择认证方式、分配角色。 API key 仅在提交时发送给宿主，保存后界面只显示「已配置」。`

**Primary action:** `新建槽位`

**Form title:** `新建槽位` / edit: `编辑 API-key 槽位`

**Steps**

1. **名称** — placeholder `例如：写码 Grok、总控`
2. **认证方式** (data-driven `SLOT_AUTH_METHODS`, equal buttons):
   - `API key` — `端点、模型与密钥；受 Pillow 完整治理。`
   - `已登录的 Grok` — `使用本机默认 Grok 登录 profile；受 Pillow 完整治理。`
   - `本机 CLI` — `可执行文件与参数；不受治理，仅供登记与账号状态查询。`
3. **角色** (`SLOT_ROLE_OPTIONS`):
   - `总控` — `可在 Pillow 中对话的主控槽位；每个项目最多一个，且必须受治理认证。`
   - `信息管理员` — `建议设置一位，负责知识与资料侧协助（不强制）。`
   - `自定义` — `自行命名角色，例如写码、审阅等。` + field placeholder `例如：写码、审阅`

**Auth-specific**

- API key: `端点 base_url`, `模型 id`, `API key` (password; placeholder `输入后不回显；保存后仅显示「已配置」`)
- Grok login: `使用本机默认 Grok 登录 profile。登录由 Grok CLI 管理，Pillow 不存储该登录。`
- Local CLI: ungoverned banner (`EXTERNAL_CLI_UNGOVERNED_*`) + executable / launch args / status args

**Actions:** `取消` / `保存` (`保存中…` while busy)

**List:** per-row `改角色` / API-key `编辑` / `启动` or `查询账号状态`

**Notices (success examples)**

- `已创建槽位。密钥已交由宿主保管，界面仅显示「已配置」。`
- `已创建槽位（使用本机默认 Grok 登录 profile）。`
- `已创建槽位（本机 CLI，不受治理）。`
- `角色已更新。` / `槽位已更新。`

**Host dual-controller error (mapped, not swallowed):**  
`该项目已有总控槽位，请先降级或删除现有总控后再指定。`  
(`runtime_slot_controller_already_exists`)

## Behaviour summary

1. **One entry:** single `新建槽位` button; form is name → auth (array-rendered) → role; submit uses **only** `gateway.createSlot(...)`.
2. **Legacy UI removed:** no `createGrokLoginSlot` / `createGrokApiKeySlot` / `createExternalCliSlot` in section source (grep: only `createSlot`). Gateway legacy methods **remain** for host registration.
3. **Role reassign:** list `改角色` → `updateSlot({ keepAuth: true, role, ... })`; no delete/recreate.
4. **No optimistic create:** list updates only after successful create + reload; failure shows `runtime-slots-error` and keeps zero new slots.
5. **Secrets:** password field; draft cleared **before** host await so failure paths never leave key in DOM; views never carry key material.

## Write-boundary audit

| Area | Result |
| --- | --- |
| Allowlist only | Yes — settings feature + desktop tests + this report |
| `crates/**` | Not modified |
| `apps/desktop/src-tauri/**` | Not modified |
| `scripts/**` | Not modified |
| Git write ops | None (no commit/add/push/reset) |
| Report hygiene | No machine absolute paths; no keys/tokens |
| Secrets in UI | Password input only; cleared on submit; never on `RuntimeSlotView` |
| Legacy gateway methods | Kept on interface + Tauri + Mock; UI does not call them |

## Residual risks

1. **Edit API-key path** still uses `updateGrokApiKeySlot` for credential fields; role on the same form then calls `updateSlot`. Two host round-trips on dual edit — fine for this ticket; could later unify fully on `update_slot`.
2. **OAuth auth kind** not added yet — catalog is ready (`SLOT_AUTH_METHODS` + one more row).
3. **Host dual-controller / CLI-controller** enforcement still authoritative; UI mirrors rules for UX but does not replace host checks.
4. **CSS** not changed (outside allowlist); reused existing form/kind classes.
5. Pre-existing jsdom noise (window resize / mock sendTurn stderr) unchanged and unrelated.

## Completion

`ONE-SLOT-FLOW-COMPLETE`
