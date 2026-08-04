# Wave C2-D Controller Acceptance Fix Brief

**Controller session:** `<keeper-session>`
**Finding owner:** controller hands-on acceptance
**Severity:** Important
**Status:** C2-D acceptance blocked pending fix and controller revalidation

## Rejected Behavior

When a portable conversation resource is opened with a real
`resourceGateway`, `Workbench.previewPathRequest` remains populated after the
right panel closes. The child integration host clears its local preview, but
reopening changes `open` to true while the old request still exists, so its
effect issues the request again and asynchronously resurrects the closed
preview.

The current “closing and reopening does not resurrect a stale preview” test
uses a tree selection and asserts only immediately after reopen. It does not
cover the persistent conversation-path request.

## Exclusive Write Whitelist

Modify only:

- `apps/desktop/src/features/workbench/Workbench.tsx`
- `apps/desktop/tests/resource-workbench-integration.test.tsx`
- `docs/execution/2026-07-31-wave-c2d-resource-workbench-integration-report.md`

All other paths are read-only. No stage or commit.

## Required Fix

Create one close helper in `Workbench` that synchronously clears:

- `resourcePaneScope`;
- `resourceSelection`;
- `previewPathRequest`.

Use that helper for:

- the active stage-header toggle's close branch;
- the contextual pane close button.

Opening remains project-scoped and must not clear unrelated conversation or
project state. Project-switch clearing remains unchanged.

## Required Regression

Add a test that:

1. renders `Workbench` with a real mock `resourceGateway` and a timeline
   containing one portable conversation resource;
2. opens that resource and awaits the first preview;
3. closes the panel;
4. reopens it without selecting any resource;
5. waits long enough for effects/promises to flush;
6. proves preview call count did not increase and old preview content did not
   reappear;
7. repeats closure using the pane close button or otherwise covers both close
   paths across the integration suite.

The pre-fix test must fail for asynchronous resurrection, not merely at a
synchronous DOM snapshot.

## Required Gates

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/resource-workbench-integration.test.tsx tests/resource-pane.test.tsx tests/project-workbench.test.tsx tests/conversation-workbench.test.tsx
pnpm --filter @pillow/desktop typecheck
git diff --check
```

Append RED/GREEN evidence, exact changed files, final counts, staging count,
and residual risk to the existing report. Worker completion remains
non-acceptance until the controller independently reruns the gates and
manually verifies both close paths.
