# Wave C2-E Resource Driver Worker Brief

Read first and treat as the exact requirements:
`docs/superpowers/plans/2026-07-31-c2e-isolation-restart-desktop-acceptance.md`.

## Role

You are grok3. Own only the project resource/recents UI helper. The
orchestrator and activity helper are parallel-owned; do not edit them.

## Exclusive write whitelist

- `scripts/lib/c2-resource-ui-acceptance.mjs`
- `docs/execution/2026-07-31-wave-c2e-resource-driver-report.md`

Do not edit any other path.

## Required exports

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

## Required behavior

- First boot Keeper as a worker for project `pillow`, task
  `wave-c2e-resource-driver`.
- Select by public sidebar accessible name and wait for the winning active
  project.
- Reach and activate `resource-toggle` by keyboard; prove focus and expanded
  contextual pane.
- Exercise root tree, debounced search, bounded Markdown preview, and recents.
- Own path/marker must be visible where expected; foreign path/marker must be
  absent from the whole resource context.
- Clear search before inspecting recents.
- Restore assertions open the own recent row and prove the own preview.
- Use only portable relative paths and public DOM; never filesystem, Tauri
  invoke, SQLite, or machine-root text.
- Helpers do not create fixtures, launch/close processes, or select a second
  project on their own.

## Focused verification

Use a syntax/import smoke if the outer orchestrator is not ready. Run
`git diff --check` on the whitelist.

## Completion contract

Write the report file with changed files, evidence assertions, tests, residual
risks, and status. Return only status, one-line test summary, concerns, and the
Keeper completion message. Worker completion is not controller acceptance.
