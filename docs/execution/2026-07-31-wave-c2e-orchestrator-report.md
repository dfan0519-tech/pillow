# Wave C2-E Orchestrator Report

**Worker**: grok1
**Task**: `wave-c2e-orchestrator`
**Role**: Real Tauri lifecycle orchestrator and outer Node E2E
**Date**: 2026-07-31
**Keeper session**: `<keeper-session>` (worker; parent `<keeper-session>`)
**Status**: `DONE_WITH_CONCERNS`

## Changed files (whitelist only)

| Path | Action |
|------|--------|
| `scripts/wave-c2-resource-acceptance.mjs` | Created — full create/restore lifecycle orchestrator |
| `apps/desktop/tests/e2e/c2-resource-workbench.spec.ts` | Created — outer Node E2E asserting frozen completion record |
| `apps/desktop/package.json` | Modified — appended `tests/e2e/c2-resource-workbench.spec.ts` to serial `test:e2e` (existing tests preserved and ordered first) |
| `docs/execution/2026-07-31-wave-c2e-orchestrator-report.md` | Created — this report |

No production Rust/React/gateway/shared-helper paths were edited. Dirty tree outside the harness temporary root was not staged, committed, reset, or cleaned.

## Orchestrator behavior

1. **Fixtures** — unique `mkdtemp` root with project A/B folders and portable markers:
   - A: `notes/a-only.md` → `PILLOW_C2_A_ONLY_MARKER`
   - B: `notes/b-only.md` → `PILLOW_C2_B_ONLY_MARKER`
2. **Register A/B** — real native acceptance create path against one shared temporary app-data directory and temporary WebView2 profile (`PILLOW_ACCEPTANCE_*` + readiness evidence).
3. **Create / restore** — relaunch with `PILLOW_RUNTIME=fake`, unique CDP port via an acceptance-owned build-time Tauri `--config` that sets window `additionalBrowserArgs`, then Playwright Core `connectOverCDP` to a real Tauri WebView2 page.
4. **Helpers (frozen imports)**:
   - `scripts/lib/c2-resource-ui-acceptance.mjs`: `selectProject`, `exerciseProjectResources`, `assertRestoredProjectRecents`
   - `scripts/lib/c2-activity-ui-acceptance.mjs`: `createConversationActivity`, `assertConversationActivityRestored`, `assertConversationActivityAbsent`
5. **Normal close** — public window control `aria-label="关闭"`; require exit status 0, no signal, clean process snapshot. Forced `ProcessSupervisor.cleanup` is safety-net only and fails the run.
6. **Terminal record** — single line:
   `KEEPER_WAVE_C2_RESOURCE_ACCEPTANCE {"schemaVersion":1,"status":"passed",...}` with every frozen field from the plan. Failures emit a redacted `status:"failed"` record (paths/UUIDs scrubbed).

## Process / temporary-directory cleanup

- Reuses `scripts/lib/desktop-process.mjs` (`ProcessSupervisor`, baseline capture, `waitForChildExit`, `waitForDesktopProcessSnapshotClean`) and `scripts/lib/desktop-profile.mjs` (real WebView2 profile isolation snapshot).
- Tracks launch PID + readiness PID; does not invent kill logic.
- Temporary root (`pillow-c2e-*`) is removed only after a clean desktop process snapshot (no `pillow.exe`, no supervised listeners).
- Real `LOCALAPPDATA` WebView2 profile is snapshotted before/after and must be unchanged.
- Nested E2E uses `enforceDesktopClean: false` so the outer supervisor does not steal the orchestrator’s process baseline proof.

## Exact test commands and results

```text
node --check scripts/wave-c2-resource-acceptance.mjs
→ exit 0 (parse/syntax OK)

node --check apps/desktop/tests/e2e/c2-resource-workbench.spec.ts
→ exit 0 (parse/syntax OK)

git diff --check -- scripts/wave-c2-resource-acceptance.mjs apps/desktop/tests/e2e/c2-resource-workbench.spec.ts apps/desktop/package.json
→ exit 0

node --test --test-concurrency=1 apps/desktop/tests/e2e/c2-resource-workbench.spec.ts
→ exit 1
  AssertionError: orchestrator exit 1
  Cause: ERR_MODULE_NOT_FOUND
  Cannot find module '…/scripts/lib/c2-resource-ui-acceptance.mjs'
  imported from scripts/wave-c2-resource-acceptance.mjs
```

Helper presence at verification time:

| Module | Present |
|--------|---------|
| `scripts/lib/c2-activity-ui-acceptance.mjs` | Yes |
| `scripts/lib/c2-resource-ui-acceptance.mjs` | **No** |

Per brief: when parallel helper files are absent, prove orchestrator imports/parses and report the integration dependency rather than editing helper files. No helper files were edited.

## Residual risks / concerns

1. **Integration dependency (blocking full green)** — `c2-resource-ui-acceptance.mjs` (grok3) must land with the frozen export signatures before the focused E2E can pass.
2. **Full lifecycle not exercised green yet** — register/create/restore/CDP/close paths are implemented but not proven end-to-end until both helpers exist and a full debug Tauri run completes on this machine.
3. **Register phases use native acceptance create** for each project (picker grant path). UI create/restore phases intentionally omit `PILLOW_ACCEPTANCE_*` so React state reloads both projects from SQLite on a fresh process.
4. **Long wall-clock** — debug `tauri build` plus four desktop process restarts; E2E timeout is 900s / nested 840s.
5. Worker completion is **not** controller acceptance; final `docs/verification/2026-07-31-wave-c2-acceptance.md` remains controller-only.

## Follow-up (2026-07-31) — CDP driver

CDP no longer uses `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` (dead under High Mandatory Level). Acceptance now rebuilds debug with an acceptance-owned temp Tauri `--config` that injects window `additionalBrowserArgs` (`--remote-debugging-port=<fixed-free-port>` plus wry defaults) and reuses that port for all four phases. See `docs/execution/2026-07-31-wave-c2e-cdp-driver-fix-report.md`.

## Keeper completion message

```text
C2-E orchestrator + outer E2E wired (whitelist only). Syntax/diff-check green.
Focused E2E red solely on missing scripts/lib/c2-resource-ui-acceptance.mjs
(activity helper present). DONE_WITH_CONCERNS — integration dependency, not
production edits. Worker completion ≠ controller acceptance.
```
