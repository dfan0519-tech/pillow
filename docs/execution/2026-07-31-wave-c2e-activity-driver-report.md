# Wave C2-E Activity Driver — Worker Report

**Date**: 2026-07-31
**Status**: `DONE`
**Worker**: grok2 (activity UI helper only)
**Task**: `wave-c2e-activity-driver`
**Keeper session**: `<keeper-session>` (worker; parent `<keeper-session>`)
**Scope**: Conversation/activity public-UI helpers only. No orchestrator, resource helper, production, stage, or commit.

---

## Changed files (whitelist only)

| Path | Role |
|------|------|
| `scripts/lib/c2-activity-ui-acceptance.mjs` | **New** — create / restore / absent activity UI drivers |
| `docs/execution/2026-07-31-wave-c2e-activity-driver-report.md` | This report |

**Not modified**: orchestrator, resource helper, desktop production, package.json, SQLite, process supervision, parent plan, final acceptance doc.

---

## Contract delivered

Frozen exports:

```js
export async function createConversationActivity({ page, prompt });
export async function assertConversationActivityRestored({
  page,
  prompt,
  foreignPrompt,
});
export async function assertConversationActivityAbsent({
  page,
  forbiddenPrompt,
});
```

### Behavior

1. **Create** — clicks accessible `新建对话`, fills `工作请求` composer, sends via `composer-send`, waits for `timeline-user-message` (prompt) and terminal `timeline-assistant-message` (`Workspace contains … Ready for deeper work.` from the committed fake-turn fixture).
2. **Activity expand** — opens real `agent-activity-disclosure` / `agent-activity-summary` / `agent-activity-details`, then asserts:
   - plan steps: `Scan files`, `Write summary` (`agent-activity-steps`);
   - tool: `list workspace root` / `list_dir` (`agent-activity-tools`), nested `输出` expanded with `crates/` or `apps/`;
   - sub-agent: `explore` + `indexed 3 top-level entries` (`agent-activity-subagents`);
   - artifacts: `workspace-summary.md` via `agent-activity-artifacts-toggle` / `agent-activity-artifacts`;
   - completed status: `已完成`.
3. **Restore** — does **not** send another turn; proves own prompt + terminal assistant + same activity; proves `foreignPrompt` absent from the page body.
4. **Absent** — proves `forbiddenPrompt` is missing from body, `timeline-user-message`, `conversation-row`, and any `agent-activity-disclosure` text (foreign conversation ownership).
5. **ID hygiene** — rejects fixture raw ids `tc-list`, `sa-explore`, `art-summary` in disclosure text; evidence is JSON-serializable booleans/titles only (no run/session/tool/sub-agent ids).
6. **Non-goals** — helpers do not select projects, launch/close processes, write files, or read SQLite/filesystem.

Selectors used: accessible roles/names (`新建对话`, `工作请求`, `发送` is available but send uses stable `composer-send`) and stable `data-testid` values only.

---

## Focused verification

Outer orchestrator and resource helper were **not present** at worker time, so full desktop E2E could not run. Syntax/import smoke and whitelist whitespace check were used.

### Commands

```powershell
node --input-type=module -e "import { createConversationActivity, assertConversationActivityRestored, assertConversationActivityAbsent } from './scripts/lib/c2-activity-ui-acceptance.mjs'; ..."
git diff --check -- scripts/lib/c2-activity-ui-acceptance.mjs docs/execution/2026-07-31-wave-c2e-activity-driver-report.md
```

### Results

| Check | Exit | Outcome |
|-------|------|---------|
| Module import | 0 | All three exports are `function` |
| Guard: missing page | 0 | `createConversationActivity` / restore throw before UI actions |
| `git diff --check` (whitelist) | 0 | No whitespace errors |

---

## Residual risks

1. **Integration not exercised** — full create/restore/absent paths need the C2-E orchestrator + real Tauri/`PILLOW_RUNTIME=fake` WebView2 page; worker cannot launch desktop (out of whitelist).
2. **Shared fake fixture** — absence checks key off the **foreign prompt** and conversation-row ownership, not plan titles alone, because the same fake-turn plan/tool labels would appear if the opposite project also sent a turn.
3. **Timeout budget** — default 45s waits assume the committed fake runtime completes promptly under real host delays; orchestrator may need a longer outer budget.
4. **Nested `输出` locator** — uses `summary` with exact text `输出` inside `agent-activity-tools`; if future chrome adds a second matching summary, the first nested details is expanded (matches current unit-test pattern).

---

## Staging / commit

- **Not staged**
- **Not committed**
- Whitelist-only intentional edits; pre-existing dirty tree left as found.

---

## Keeper completion message

```
Pillow C2-E activity driver complete (unstaged). scripts/lib/c2-activity-ui-acceptance.mjs exports createConversationActivity, assertConversationActivityRestored, assertConversationActivityAbsent. Public UI only: 新建对话 + composer, expand agent-activity-disclosure, assert fake-turn plan/tool/sub-agent/artifact/completed; restore without resend; foreign prompt/activity absence. Smoke: import 3/3 + git diff --check pass. Outer E2E depends on parallel orchestrator. No production edits, stage, or commit. Worker completion ≠ controller acceptance.
```
