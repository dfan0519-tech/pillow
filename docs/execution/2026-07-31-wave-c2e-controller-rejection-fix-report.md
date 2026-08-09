# Wave C2-E Controller Rejection Fix Report

**Worker**: grok-c2e-fix
**Local session note**: `<keeper-session>` (controller boot; worker role required local parent session id)
**Date**: 2026-07-31
**Baseline HEAD**: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
**Status**: `DONE` — focused C2-E E2E green; four phases complete without gate relaxation

## Changed files (whitelist only)

| Path | Action |
|------|--------|
| `scripts/lib/c2-resource-ui-acceptance.mjs` | Search locator + content query + recents remount |
| `scripts/wave-c2-resource-acceptance.mjs` | Strict knownPids residual, isolated `CARGO_TARGET_DIR`, WebView2 profile cmdline proof, close exact match, restore conversation open |
| `apps/desktop/tests/e2e/c2-resource-workbench.spec.ts` | Unchanged (still asserts frozen completion record) |
| `docs/execution/2026-07-31-wave-c2e-controller-rejection-fix-report.md` | This report |

No production Rust/React/CSS/Cargo/package files modified. Nothing staged or committed. Dirty C1/C2 worktree left intact.

## Rejection items and fixes

### A. Search result locator

**Root cause (two layers):**

1. Pane-wide `textContent.includes(ownPath)` false-positive: after a tree open the bounded **preview** already contains `ownPath`, so the wait settled before search hit buttons existed.
2. Kernel search is **content-based** (line text), not path-name matching. Querying the leaf `a-only.md` never produced hits for fixtures whose body is only the marker.

**Fix:**

- Public contract: search hit `aria-label` = `` `${path} 第 ${line} 行` ``.
- Wait for a `搜索结果` button whose aria-label matches that contract for `ownPath`.
- Fill search with `ownMarker` (fixture content), not the leaf path name.
- Assert path presence inside the search region only (not whole pane).

### B. Strict zero residual

**Root cause:** Prior close path dropped `ProcessSupervisor.knownPids` natural-exit checks and treated orphan utilities as ignorable. Also, third-party injectors (e.g. `SGTool.exe`) were absorbed into the parent PID tree.

**Fix:**

- Click public window control `aria-label="关闭"` with **`exact: true`** (avoids matching `关闭资源面板`).
- Require root exit status 0, no signal.
- Capture supervised tree before close; **retain only acceptance-managed PIDs** (pillow.exe, msedgewebview2 / WebView2, processes whose cmdline references acceptance exe / temp profile / CDP port).
- Wait up to 60s for managed knownPids to exit **naturally**.
- Any survivor → safety-net `cleanup` **and fail** (forced cleanup never counts as pass).
- CDP port release + desktop process snapshot still required.
- Orphan **WebView2** utilities that remain managed are **not** ignored.

### C. Isolated acceptance binary

**Root cause:** Debug build wrote into repository `target/debug/pillow.exe`, baking the temporary CDP port into the shared debug artifact.

**Fix:**

- Set acceptance-owned temp `CARGO_TARGET_DIR` under the `mkdtemp` root.
- Launch `pillow.exe` only from that directory.
- Delete the temporary root (including cargo-target) only after zero residual pillow processes.
- Hard guard refuses launching repository `target/debug/pillow.exe`.

### D. WebView2 temp profile proof

**Root cause:** Readiness JSON only echoes `WEBVIEW2_USER_DATA_FOLDER`; that is not runtime proof.

**Fix:**

- On first launch (register-a), snapshot Win32 processes with **CommandLine**.
- Require a live `msedgewebview2.exe` (or WebView2-named) process whose cmdline proves `--user-data-dir` under the acceptance temp profile.
- Emit `KEEPER_WAVE_C2_RESOURCE_ACCEPTANCE_WEBVIEW_PROFILE` diagnostic (path redacted; `profileScope: "acceptance-temp"`).
- Real LocalAppData WebView2 profile snapshot still must be unchanged (not weakened).

### Additional create/restore integration fixes

| Issue | Fix |
|-------|-----|
| Recents UI not refreshed after open | Collapse+expand host pane via public keyboard path remounts browser so `listRecentProjectResources` reloads SQLite recents |
| Restore activity not visible | Project select clears conversation selection; open `conversation-row` (title often remains `新对话` on real SQLite) then assert timeline |
| Activity chrome collision | Collapse resource pane before conversation turn |

## Verification

```text
node --check scripts/wave-c2-resource-acceptance.mjs
→ exit 0

node --check scripts/lib/c2-resource-ui-acceptance.mjs
→ exit 0

pnpm --filter @pillow/desktop typecheck
→ exit 0

git diff --check -- scripts/wave-c2-resource-acceptance.mjs scripts/lib/c2-resource-ui-acceptance.mjs apps/desktop/tests/e2e/c2-resource-workbench.spec.ts
→ exit 0

node --test --test-concurrency=1 apps/desktop/tests/e2e/c2-resource-workbench.spec.ts
→ exit 0
  ✔ Wave C2-E proves two-project isolation, restart recovery, and normal close over real Tauri WebView2 (~148s)
  pass 1 / fail 0
```

HEAD remained `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`. Staging count 0.

### Focused E2E progression (no assertion relaxation)

| Milestone | Result |
|-----------|--------|
| CDP + register-a/b (prior) | Already green via `additionalBrowserArgs` |
| After search content + locator fix | Create resource tree/search green |
| After recents remount | Recents green |
| After exact window close | Create close green |
| After managed knownPids filter (drop SGTool) | Residual green |
| After open conversation-row on restore | Restore activity green |
| Full four-phase E2E | **PASS** |

## Residual risks

1. **Full rebuild each run** — acceptance `CARGO_TARGET_DIR` is temp and deleted; ~100s debug compile wall-clock.
2. **Third-party injectors** — filtered from managed residual; if an injector is mis-classified as managed, residual may fail closed (correct).
3. **Conversation title** — real SQLite keeps `新对话` unless product renames on send; restore opens first/default row then proves prompt on the timeline.
4. **Worker completion ≠ controller acceptance** — final C2 checkpoint remains controller-owned.
5. **Shared host WebView2 processes** may exist outside acceptance; only tree/profile-bound processes are managed.

## Staging / commit

- **Not staged**
- **Not committed**
- Whitelist-only intentional edits

## Keeper completion

```text
C2-E controller rejection fix: search locator uses public aria-label contract
+ content marker query; strict managed knownPids residual (WebView2 not
ignorable; forced cleanup fails); acceptance CARGO_TARGET_DIR isolation;
WebView2 user-data-dir proven from live process cmdline. Four-phase focused
E2E green (~148s). Gates: node --check x2, desktop typecheck, git diff --check
green. HEAD 7b7f70f. DONE. No stage/commit. Worker ≠ controller acceptance.
```
