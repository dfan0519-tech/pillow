# Wave C2-D Resource Workbench Integration Brief

**Controller session:** `<keeper-session>`
**Stage:** Stage 1 / C2 Task 2
**Repository:** `<workspace>\pillow`
**Baseline HEAD:** `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
**Status:** implementation task; acceptance belongs to the controller

## Objective

Mount the already accepted project resource browser and bounded preview into
the existing right contextual region of the Pillow workbench. The conversation
remains the visual center. The integration must discard every stale browse or
preview result when the active project changes.

This task wires accepted components; it does not add new resource capabilities.

## Frozen Scope

This task may:

- inject one `TauriResourceGateway` from `main.tsx`;
- pass an optional typed resource gateway through `App` to `Workbench`;
- adapt the existing right `features/workbench/ResourcePane` into the single
  integration host for browser, preview, and existing conversation references;
- add a controlled-open mode to the accepted
  `features/resources/ResourcePane` while preserving its current standalone
  uncontrolled contract;
- add focused DOM/integration tests and the smallest contextual-panel styles.

This task must not:

- change Rust, Tauri commands, DTO contracts, SQLite, or preview classification;
- change conversation timeline/activity projection;
- add file writes, terminal, Git, Obsidian, keys, MCP, network, or runtimes;
- create a second sidebar, dashboard, task table, or global resource store;
- expose project roots, native paths, handles, raw bytes, session IDs, or
  debug metadata;
- touch `<workspace>\VaultSystem\keeper`;
- stage or commit.

## Exclusive Write Whitelist

Modify only:

- `apps/desktop/src/App.tsx`
- `apps/desktop/src/main.tsx`
- `apps/desktop/src/features/workbench/Workbench.tsx`
- `apps/desktop/src/features/workbench/ResourcePane.tsx`
- `apps/desktop/src/features/resources/ResourcePane.tsx`
- `apps/desktop/src/styles/workbench-chrome.css`
- `apps/desktop/src/styles/resource-pane.css`
- `apps/desktop/tests/resource-pane.test.tsx`
- `apps/desktop/tests/project-workbench.test.tsx`

Create only:

- `apps/desktop/tests/resource-workbench-integration.test.tsx`
- `docs/execution/2026-07-31-wave-c2d-resource-workbench-integration-report.md`

All other files are read-only. Preserve the existing dirty C1/B3/C2 working
tree; do not revert, reset, format wholesale, stage, or commit.

## Required Interfaces

`App`:

```ts
type AppProps = {
  gateway: ProjectGateway;
  conversationGateway?: ConversationGateway;
  resourceGateway?: ResourceGateway;
  // existing props unchanged
};
```

`main.tsx` creates exactly one `TauriResourceGateway` and passes it to `App`.

`Workbench` accepts:

```ts
resourceGateway?: ResourceGateway;
```

When a gateway and active project exist, the right contextual region receives
only the active `project.id`, the gateway, and portable callback paths. When
either is absent, no resource-gateway method may be invoked.

The accepted project browser gains a backwards-compatible controlled mode:

```ts
type ResourcePaneProps = {
  projectId: string;
  gateway: ResourceGateway;
  onOpenResource: (relativePath: string) => void;
  open?: boolean;                 // undefined => existing internal state
  onOpenChange?: (open: boolean) => void;
  showToggle?: boolean;           // defaults true
};
```

Existing isolated usage and tests must keep the current closed-by-default
behavior. The workbench host uses `open={true}` and `showToggle={false}` so the
existing stage-header icon remains the only outer disclosure control.

## Integration State Rules

The existing stage-header resource button:

- remains closed by default;
- is disabled when no project is active;
- opens the one existing right contextual region;
- closes it without mutating project or conversation state.

The integration host owns only local contextual state:

```ts
type PreviewScope = {
  projectId: string;
  generation: number;
};
```

Before every preview request:

1. increment generation;
2. clear prior preview and prior preview error synchronously;
3. capture `{ projectId, generation }`;
4. call `previewProjectResource({ projectId, relativePath })`;
5. commit success/error only if both active project and generation still match.

On project change or unmount, increment generation and clear preview,
selection, loading, and error before any result from the prior project can
render.

Portable paths selected from tree, search, recents, or a compatible
conversation resource call the same preview function. Existing conversation
reference fallback remains available when `resourceGateway` is omitted, so
prior `conversation-workbench` behavior does not regress.

## Visual and Interaction Rules

- The conversation keeps `minmax(0, 1fr)` and remains the dominant column.
- The right panel stays contextual and collapsible; no permanent cards or
  dashboard chrome.
- The browser and preview share one scrollable right region.
- Loading, empty, binary, truncation, and opaque error states are inline.
- Never display a host/root path. Only `ResourcePreview.path` is user-visible.
- Browser controls and preview content remain keyboard reachable.
- Do not add a second “Resources” disclosure inside the open right panel.

## Test-First Sequence

1. Write focused integration tests and controlled-mode regression tests.
2. Run them before production changes and record RED.
3. Implement the smallest wiring change.
4. Run all required gates and write the report.

Required focused coverage:

- panel is closed by default and no resource request occurs before opening;
- no active project means the outer toggle is disabled and gateway calls stay
  at zero;
- opening with project A calls list/recents only with project A;
- selecting tree, recent, and search portable paths calls
  `previewProjectResource` with active project A and renders the accepted
  preview;
- a pending project-A preview is cleared before project B renders;
- late project-A success and failure cannot appear under project B;
- opening B and selecting a B path calls only project B;
- closing/reopening does not resurrect stale preview;
- `resourceGateway` omission preserves the existing conversation-reference
  fallback tests;
- the controlled project browser has no duplicate inner disclosure toggle;
- no machine root, raw bytes, or second-dashboard markers appear.

## Required Gates

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/resource-workbench-integration.test.tsx tests/resource-pane.test.tsx tests/project-workbench.test.tsx tests/conversation-workbench.test.tsx
pnpm --filter @pillow/desktop typecheck
git diff --check
```

All must exit `0`. The Worker report must list RED evidence, exact changed
files, final test counts, staging count, residual processes, and remaining
risks. Post a Keeper completion message to controller session
`<keeper-session>`, then wait for controller capture and lease
release.

Worker completion is not acceptance. The controller will inspect the complete
diff, execute adversarial project-switch checks, rerun the gates personally,
and decide whether the task may proceed to independent Codex2 review.
