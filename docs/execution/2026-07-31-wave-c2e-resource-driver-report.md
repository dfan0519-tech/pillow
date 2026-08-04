# Wave C2-E Resource Driver — Completion Report

**Date**: 2026-07-31
**Worker**: grok3
**Role**: project resource / recents UI helper only
**Task**: `wave-c2e-resource-driver`
**Keeper worker session**: `<keeper-session>`
**Status**: `DONE_WITH_CONCERNS`

Worker completion is **not** controller acceptance.

---

## Changed files (exclusive whitelist)

| Path | Action |
|------|--------|
| `scripts/lib/c2-resource-ui-acceptance.mjs` | **Created** — resource isolation + recents Playwright helpers |
| `docs/execution/2026-07-31-wave-c2e-resource-driver-report.md` | **Created** — this report |

No other paths edited. Orchestrator (`scripts/wave-c2-resource-acceptance.mjs`) and activity helper (`scripts/lib/c2-activity-ui-acceptance.mjs`) were not present / not modified.

---

## Required exports delivered

```js
export async function selectProject(page, projectName);
export async function exerciseProjectResources({
  page,
  ownPath,
  ownMarker,
  foreignPath,
  foreignMarker,
});
export async function assertRestoredProjectRecents({
  page,
  ownPath,
  ownMarker,
  foreignPath,
  foreignMarker,
});
```

Helpers return JSON-serializable evidence objects only.

---

## Evidence assertions (behavior contract)

### `selectProject(page, projectName)`

- Clicks the public sidebar button whose accessible name equals `projectName` (`aria-label` on project rows under `data-testid="app-sidebar"`).
- Waits until that row is `aria-pressed="true"` **and** `data-testid="active-project-name"` contains the winning project name.
- Evidence: `{ selectedProjectName, activeProjectName, sidebarPressed }`.

### `exerciseProjectResources({ page, ownPath, ownMarker, foreignPath, foreignMarker })`

Assumes the orchestrator already selected the target project (does not select a second project).

1. **Keyboard open** — focus `data-testid="resource-toggle"`, activate with Enter; prove focus, `aria-pressed="true"`, visible `data-testid="resource-pane"`, labelled `上下文资源`, region `项目资源`.
2. **Root tree** — expand portable directory segments via labelled buttons; open own file; prove `data-testid="resource-preview"` shows `ownPath` + `ownMarker`.
3. **Debounced search** — type own leaf name into `搜索项目资源`; wait for public `搜索结果` region settled (not `搜索中…`) containing `ownPath`; foreign path/marker absent; open hit; re-prove own preview.
4. **Clear search before recents** — empty search input; wait until search region is gone and `最近打开` returns.
5. **Recents** — own recent row by exact accessible name `ownPath` present; foreign path/marker absent from recents and whole resource context; open own recent; prove own preview again.
6. **Isolation** — foreign path/marker asserted absent from the entire resource pane after each major step.
7. **Portable only** — argument guards reject drive roots, backslashes, absolute `/`, `..`, and URL schemes. No filesystem, Tauri invoke, SQLite, or machine-root text is used.

Evidence includes keyboard/pane flags, tree/search/recents visibility, and foreign-absent flags.

### `assertRestoredProjectRecents({ page, ownPath, ownMarker, foreignPath, foreignMarker })`

- Opens pane by keyboard (same focus/expand proof).
- Clears search if non-empty so recents (not search hits) are inspected.
- Proves own recent row present; foreign recent row count is 0; foreign path/marker absent from recents and resource context.
- Opens own recent row; proves own bounded preview path + marker.

---

## Focused verification

Outer orchestrator / full Tauri E2E was **not ready** (no `scripts/wave-c2-resource-acceptance.mjs` at worker time). Ran syntax/import smoke instead.

### Commands

```text
node --input-type=module -e "<import all three exports; portable-path guard rejects F:\ machine root>"
git diff --check -- scripts/lib/c2-resource-ui-acceptance.mjs
```

### Results

| Check | Result |
|-------|--------|
| Import/export smoke (3 functions) | **PASS** |
| Portable path fail-closed on machine-root `ownPath` | **PASS** |
| `git diff --check` on helper | **PASS** (exit 0) |
| Full C2-E desktop E2E | **Not run** — orchestrator parallel-owned / not present |

---

## Residual risks

1. **No live WebView2 exercise** — helper logic is wired to public DOM contracts from C2-C1/C2-D tests, but end-to-end proof waits on the orchestrator attaching CDP and sequencing A/B phases.
2. **Search query uses leaf name** — content-index behavior is host-dependent; leaf path tokens are the stable portable selector. Marker proof is on the bounded preview after open.
3. **Directory aria-labels are leaf names** — multi-segment trees expand by leaf segment; unusual duplicate sibling directory names could be ambiguous (fixture layout uses distinct `notes/…` paths).
4. **Recents depend on prior open** — create-phase `exerciseProjectResources` opens the file (tree + search + recent) so a recent row exists; restore-phase assumes durable recents from that prior open.
5. **Parallel workers** — activity helper and orchestrator may still land; integration mismatches (selector drift, timing) are controller/fixer territory, not fixed here.

---

## Staging / commit

- **Not staged**
- **Not committed**
- Whitelist-only intentional edits; dirty tree otherwise preserved

---

## Keeper completion message

```text
Pillow C2-E resource driver WORKER COMPLETE (unstaged; acceptance belongs to controller).

Created scripts/lib/c2-resource-ui-acceptance.mjs with selectProject,
exerciseProjectResources, and assertRestoredProjectRecents. Public sidebar +
keyboard resource-toggle + tree/search/preview/recents isolation using portable
relative paths only. Foreign path/marker asserted absent from resource context;
search cleared before recents; restore opens own recent and proves own preview.

Focused: import/export smoke PASS; portable-path guard PASS; git diff --check PASS.
Full desktop E2E not run (orchestrator parallel-owned / not present). Staging 0.
No production edits. Awaiting controller assembly + focused E2E.
```
