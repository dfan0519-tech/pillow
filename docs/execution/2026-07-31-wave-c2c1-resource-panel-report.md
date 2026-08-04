# Wave C2-C1 — Collapsible Resource Context Panel — Completion Report

**Date**: 2026-07-31
**Status**: `DONE`
**Worker**: grok-c2c1 (Grok CLI, leased)
**Scope**: Isolated renderer slice only; workbench wiring deferred to controller after activity/preview slices.
**Commit**: not staged, not committed (per brief).

---

## Changed files (whitelist only)

| Path | Role |
|------|------|
| `apps/desktop/src/features/resources/ResourcePane.tsx` | **New** — collapsible panel: open/close, recents, root tree, debounced search |
| `apps/desktop/src/features/resources/ResourceTree.tsx` | **New** — concise expandable tree rows via `ResourceGateway` |
| `apps/desktop/src/features/resources/resourcePaneState.ts` | **New** — pure helpers: sort, portable path checks, opaque errors, debounce ms |
| `apps/desktop/src/features/resources/resourceTypes.ts` | **New** — panel props and snapshot types |
| `apps/desktop/src/styles/resource-pane.css` | **New** — `ctx-resource-*` styles (no collision with conversation resource chrome) |
| `apps/desktop/tests/resource-pane.test.tsx` | **New** — focused TDD suite (8 cases) |
| `docs/execution/2026-07-31-wave-c2c1-resource-panel-report.md` | This report |

**Not modified**: `App.tsx`, `Workbench.tsx`, `ConversationTimeline.tsx`, existing CSS, gateways, Tauri, C2-A/B sources, package files.

**Note**: Existing `features/workbench/ResourcePane.tsx` (conversation reference chrome) is untouched. This slice lives under `features/resources/` and uses the `ctx-resource-pane` class prefix.

---

## Contract delivered

`ResourcePane` props:

- `projectId: string`
- `gateway: ResourceGateway`
- `onOpenResource(relativePath: string)`

Behavior:

1. **Closed by default** — labelled toggle (`打开资源面板` / `关闭资源面板`); body absent until open.
2. **On open** — loads recents then deterministic root tree (`directory` before `file`, path order).
3. **Search** — explicit input; debounce `RESOURCE_SEARCH_DEBOUNCE_MS` (250ms); cancelled on query change / project switch via generation counters.
4. **Project isolation** — every request includes `projectId`; changing `projectId` clears rows/search/errors and invalidates in-flight generations before the next load.
5. **Portable paths only** — selection callbacks and filtered rows use portable relative paths; no roots/native paths accepted or emitted.
6. **Inline states** — loading / empty / error inside the panel; no cards, counters, or dashboard chrome.
7. **A11y** — focusable toggle, labelled search, labelled resource rows; `role="alert"` for opaque failures.
8. **Opaque errors** — path-bearing host messages replaced with `无法加载资源。` (never surface machine paths).

---

## TDD evidence

### RED (implementation absent)

| Command | Exit | Outcome |
|---------|------|---------|
| `pnpm --filter @pillow/desktop exec vitest run tests/resource-pane.test.tsx` | 1 | `Failed to resolve import "../src/features/resources/ResourcePane"` — suite failed before any test body |

Test file was authored **before** production modules.

### GREEN (after implementation)

| Command | Exit | Outcome |
|---------|------|---------|
| `pnpm --filter @pillow/desktop exec vitest run tests/resource-pane.test.tsx` | 0 | **8** tests passed |
| `pnpm --filter @pillow/desktop typecheck` | 0 | Clean |
| `git diff --check` (scoped whitelist paths) | 0 | No whitespace errors |

### Final verification (re-run at completion)

```
pnpm --filter @pillow/desktop exec vitest run tests/resource-pane.test.tsx
  → 8 passed

pnpm --filter @pillow/desktop typecheck
  → pass

git diff --check -- apps/desktop/src/features/resources \
  apps/desktop/src/styles/resource-pane.css \
  apps/desktop/tests/resource-pane.test.tsx
  → pass
```

### Behavior covered by tests

- Closed-default disclosure and labelled open/close control
- Open → recents region + project files region; directory-before-file root order
- Project switch isolation; stale in-flight project A results do not clobber project B
- `onOpenResource` only with portable relative paths (recents, tree children, search hits)
- Debounced search + cancel on project switch (stale hit discarded)
- Keyboard reachability (toggle → search → labelled rows)
- Opaque error display (no `F:\` / machine path leakage)
- Empty inline state without dashboard chrome

---

## Residual risks / known limitations

1. **Not wired into the workbench** — controller must mount `features/resources/ResourcePane` after activity/preview slices; conversation still uses `workbench/ResourcePane` for reference chrome only.
2. **CSS not in `main.tsx` chain** — `ResourcePane` imports `resource-pane.css` self-contained; when wired, avoid double-import if also added to the shell stylesheet graph.
3. **Search errors share `errorMessage` with browse** — last failure wins for display; both remain opaque.
4. **Directory expand state is local** — remounted on `projectId` key; deep expansion is not persisted across project switches (intentional isolation).
5. **No read/preview in this slice** — open callback only; preview pane is a later wave.
6. **Gateway does not accept AbortSignal** — cancellation is generation-based discard, not transport abort.

---

## Staging / commit

- **Not staged**
- **Not committed**
- Whitelist-only intentional edits; pre-existing C1/C2-A/B dirty tree left as found
