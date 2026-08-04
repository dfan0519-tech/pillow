# Wave C2-D Project Selection Race Fix Brief

**Controller session:** `<keeper-session>`
**Source:** Codex2 independent review; controller confirmed
**Severity:** Important
**Status:** C2-D remains rejected until controller revalidation

## Finding

`App.selectProject` commits every asynchronous `gateway.selectProject` success
and error without proving it is still the newest selection. A delayed project A
request can therefore overwrite a newer completed project B selection, after
which the newly mounted resource gateway legitimately scopes its calls to A.

`busy`-disabled buttons reduce the normal UI window but are not an authority or
concurrency boundary. Default-selection, batched discrete events, tests, and
future callers must still obey last-request-wins.

## Exclusive Write Whitelist

Modify only:

- `apps/desktop/src/App.tsx`
- `apps/desktop/tests/project-workbench.test.tsx`
- `apps/desktop/tests/resource-workbench-integration.test.tsx`
- `docs/execution/2026-07-31-wave-c2d-resource-workbench-integration-report.md`

All other paths are read-only. No stage or commit.

## Required Fix

Add an independent project-activation generation ref in `App`. Every
asynchronous project selection/activation captures a generation before its
gateway call.

Only the current generation may:

- `upsertProject`;
- `setActiveProject`;
- set or clear the project error;
- clear its `busy` state.

A stale success or stale failure returns silently and cannot alter the selected
project, resource scope, error banner, conversation selection, or busy state
owned by the latest request.

Apply the guard to both direct existing-project selection and the
`activateProject` path used after opening/registering a folder. Do not change
gateway contracts or create a global store.

## Required RED/GREEN Tests

Use public App behavior. If ordinary helpers flush the first event before the
second, dispatch two project button events inside one React `act` batch so both
handlers execute before the busy DOM update.

1. Delayed success:
   - start A, then B;
   - resolve B first and prove B is active;
   - resolve A later;
   - prove B remains active, no A error/content appears;
   - open resources and prove list/recents/preview calls are B-scoped only.
2. Delayed stale failure:
   - start A, then B;
   - resolve B;
   - reject A with a recognizable project error;
   - prove B remains active, no stale error banner appears, and B resources
     remain usable.
3. Existing single-selection and open/register flows remain green.

The pre-fix tests must fail by showing the late A result/error can mutate App
state. Record exact RED output.

## Required Gates

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/project-workbench.test.tsx tests/resource-workbench-integration.test.tsx
pnpm --filter @pillow/desktop exec vitest run tests/resource-pane.test.tsx tests/resource-preview.test.tsx tests/resource-gateway.test.ts tests/conversation-workbench.test.tsx
pnpm --filter @pillow/desktop typecheck
pnpm --filter @pillow/desktop build:ui
git diff --check
```

Append RED/GREEN evidence, changed files, staging count, and residual risk to
the existing report. The controller will independently inspect and rerun the
race tests and all gates before any acceptance.
