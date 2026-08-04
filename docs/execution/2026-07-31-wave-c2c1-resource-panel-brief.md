# Wave C2-C1 — Collapsible Resource Context Panel

## Worker and scope

One leased Grok CLI worker implements this isolated renderer slice after C2-B
acceptance. The controller will wire it into the workbench only after the
activity and preview slices arrive.

The panel is contextual and collapsible, never a permanent resource dashboard.
The conversation remains the visual centre.

## Exact write whitelist

- `apps/desktop/src/features/resources/ResourcePane.tsx` (new)
- `apps/desktop/src/features/resources/ResourceTree.tsx` (new)
- `apps/desktop/src/features/resources/resourcePaneState.ts` (new)
- `apps/desktop/src/features/resources/resourceTypes.ts` (new)
- `apps/desktop/src/styles/resource-pane.css` (new)
- `apps/desktop/tests/resource-pane.test.tsx` (new)
- `docs/execution/2026-07-31-wave-c2c1-resource-panel-report.md` (new)

Do not change `App.tsx`, `Workbench.tsx`, `ConversationTimeline.tsx`, existing
CSS, gateways, Tauri, C2-A/B files, or package files. Do not stage or commit.

## Contract

`ResourcePane` accepts a `projectId`, a `ResourceGateway`, and an
`onOpenResource(relativePath)` callback. It has no filesystem access and never
accepts or emits roots/native paths.

- Closed by default, with a labelled button to open/close.
- On open, show recents then deterministic root tree; search is explicit and
  debounced/cancellable in component state.
- Requests always include `projectId`; changed `projectId` clears in-flight
  results and state before issuing the new request.
- Tree and search rows are concise; selection calls `onOpenResource` with only
  the portable relative path.
- Loading/empty/error states are inline in the panel. Do not create cards,
  counters, or a second dashboard.
- Keyboard operation and focus labels are required; a failed resource request
  must not expose a machine path.

## Test-first and verification

Create the focused test file first and run it RED before components. Cover
closed-default disclosure, project switching isolation, recents/tree/search,
portable callback-only paths, keyboard reachability, and opaque error display.

Run:

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/resource-pane.test.tsx
pnpm --filter @pillow/desktop typecheck
git diff --check
```

The report must list changed files, RED/green evidence, residual risks, and
confirm no stage/commit.
