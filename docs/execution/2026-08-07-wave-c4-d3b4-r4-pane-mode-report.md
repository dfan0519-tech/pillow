# C4 D3B4 Round 4 — Pane mode regression fix

**Worker:** grok3  
**Controller session:** `<session-id>`  
**Worker session:** `<session-id>`  
**Date:** 2026-08-07  
**Scope:** Restore C2 resource browser as the default contextual-pane body; make Git Changes an explicit mode.

## Defect

D3B4 constructed a non-empty `gitChanges` model whenever a project + Git gateway were present, and `ResourcePane` treated that as the default body. Opening the context pane painted `aria-label="工作区变更"` instead of `region 项目资源`, with no way back to resources.

RED (controller serial desktop E2E):

- C2-E: `region 项目资源` timeout
- C3-F: opened resource must reference governed-write path/marker

## Changed files

| Path | Change |
|------|--------|
| `apps/desktop/src/features/workbench/ResourcePane.tsx` | Explicit `ContextBodyMode`; compact 资源/变更 tablist; Changes body only when mode is `changes`; Terminal/Diff still absolute priority |
| `apps/desktop/src/features/workbench/Workbench.tsx` | Owns `contextBodyMode` (default `resources`); resets on project change / pane close; notifies App with request project id |
| `apps/desktop/src/App.tsx` | Discovery/status only when `gitChangesRequestProjectId === activeProject.id`; ownership guard retained; model null outside explicit Changes |
| `apps/desktop/tests/resource-pane.test.tsx` | Default-resources + explicit-mode + Terminal/Diff precedence tests |
| `apps/desktop/tests/c4-git-changes-integration.test.tsx` | Discovery deferred until Changes; `openGitChangesMode` helper; project isolation updated |
| `scripts/lib/c4-git-changes-acceptance.mjs` | `openGitChangesPane` explicitly selects Changes mode after open |
| `docs/execution/2026-08-07-wave-c4-d3b4-r4-pane-mode-report.md` | This report |

No whitelist expansion. No CSS file changes (compact mode switch uses existing `chrome-btn` + inline layout). No Tauri commands, DTO fields, or error codes added.

## Design compliance (section 3)

1. **Explicit mode selection** — Header tablist `资源` / `变更` (`role="tablist"`, `data-testid="context-body-mode-switcher"`).
2. **Default is resources** — `contextBodyMode` defaults to `"resources"`; opening the pane shows the resource body.
3. **Terminal / write Diff absolute priority** — Unchanged order: Terminal → write Diff → (Changes \| resources). Mode switcher hidden under Terminal/Diff.
4. **Keyboard-reachable, compact, not a second dashboard** — Tabs are focusable buttons with Enter activation; live in the pane header only; no conversation-stage chrome.
5. **`region 项目资源` visible by default** — Covered by unit tests with resource gateway present.
6. **Ownership guard retained** — `gitScopeRef.current.projectId === activeProject.id` still required; project switch clears discovery/status/path/diff/loading/error; request id must match active project before discovery.
7. **Fixed public copy** — Labels: `资源`, `变更`, `工作区变更`, `加载变更…`. No paths/repo roots/argv/cwd/env/session/run/proposal ids.
8. **No new Tauri/DTO/error surface** — Presentation-only wiring.

Additional: **Git discovery/status only when Changes is selected** for the matching project id. Resource mode is unaffected by Git state (model is `null` and body stays Files/Preview).

## New / updated tests (required proofs)

| # | Proof | Where |
|---|--------|--------|
| 1 | Default open → resource body, `region 项目资源` | `resource-pane.test.tsx` |
| 2 | `gitChanges` present → still default resources | `resource-pane.test.tsx` |
| 3 | Explicit 变更 → `工作区变更` / git-changes-pane | `resource-pane.test.tsx`, integration |
| 4 | Terminal + write Diff still outrank Changes | both test files |
| 5 | Project switch isolation not weakened | integration (render-time owner, late result drop, clear-on-switch) |

## Gate results

| Command | Exit code |
|---------|-----------|
| `corepack pnpm --filter @pillow/desktop exec vitest run tests/resource-pane.test.tsx tests/c4-git-changes-integration.test.tsx tests/c4-execution-primitives.test.tsx tests/write-approval-workbench.test.tsx --no-cache` | **0** (4 files, **75/75** tests) |
| `corepack pnpm --filter @pillow/desktop typecheck` | **0** |
| `node --check scripts/lib/c4-git-changes-acceptance.mjs` | **0** |
| `git diff --check` | **0** (LF/CRLF warnings only on pre-existing dirt) |
| `git diff --cached --name-only` | **0**, empty (staging clean) |

Vitest breakdown:

- `c4-execution-primitives.test.tsx` — 28 passed  
- `resource-pane.test.tsx` — 18 passed  
- `write-approval-workbench.test.tsx` — 16 passed  
- `c4-git-changes-integration.test.tsx` — 13 passed  

## Staging / git hygiene

- Staging area empty.
- No `git add` / commit / stash / reset / clean / checkout restore.
- No desktop E2E run.
- No evidence files under `.superpowers/c4-git-acceptance/` touched.
- No process kill by PID/name.

## Residual risk

- Full serial desktop E2E (C2-E, C3-F, C4 git acceptance) is controller-only; worker did not run it.
- Mode-switch chrome uses minimal inline styles (CSS not on whitelist); visual density may need a later design-system pass.
- Leaving Changes and re-entering re-triggers discovery (intentional “才触发”); slightly more gateway traffic than the eager Round 2B path.
- Acceptance helper now requires the Changes tab to be visible (Git gateway present); scenarios without a gateway never used this helper for Git assertions.

## Explicitly not done

- No D3B4 feature work beyond the pane-mode regression.
- No desktop E2E / `wave-*-acceptance.mjs` runs.
- No native/Tauri/git-executor changes.
- No commit or staging.
- Controller source review and serial desktop E2E remain the acceptance gate.

---

Worker self-report is not acceptance.
