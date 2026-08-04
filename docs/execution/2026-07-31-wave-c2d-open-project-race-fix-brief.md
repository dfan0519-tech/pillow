# Wave C2-D — Open/Register Project Last-Request-Wins Fix Brief

**Date**: 2026-07-31
**Controller session**: `<keeper-session>`
**Baseline HEAD**: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
**Status**: controller rejection follow-up; implementation required
**Commit policy**: do not stage or commit

## Context

The direct existing-project selection path now has a project-activation
generation guard. The controller rejected the fix as incomplete because
`openProject()` still:

1. obtains its generation only later, inside `activateProject()`, after the
   directory picker and open/register request;
2. writes project errors unconditionally in its outer `catch`;
3. clears `busy` unconditionally in its outer `finally`.

Therefore an older open/register action may become authoritative after a newer
selection, paint a stale error, or clear busy state owned by the newer action.
The UI's disabled state is not an authority boundary.

## Exclusive Write Whitelist

Modify only:

- `apps/desktop/src/App.tsx`
- `apps/desktop/tests/project-workbench.test.tsx`
- `apps/desktop/tests/resource-workbench-integration.test.tsx`
- `docs/execution/2026-07-31-wave-c2d-resource-workbench-integration-report.md`

All other paths are read-only. Preserve the complete dirty working tree. Do not
stage, commit, reset, restore, or edit `<workspace>\VaultSystem\keeper`.

## Required Semantics

- Capture the project-activation generation at the start of the user action,
  before `pickDirectory()` or any other asynchronous gateway call.
- Pass that captured generation through the open/register/activate flow. Do not
  allocate a newer generation only when `activateProject()` is reached.
- Only the current generation may:
  - call the final `gateway.selectProject()` for the opened project;
  - upsert or activate a project;
  - set or clear the project error;
  - clear busy state owned by project activation.
- A stale open/register success or failure returns silently. It must not
  supersede a newer direct selection, invoke a late final selection, paint an
  error banner, clear current busy state, or remount the resource scope.
- A current picker cancellation still clears its own busy state and preserves
  the current active project.
- Preserve the existing direct-selection guards and all conversation/resource
  scope protections. Do not change gateway contracts or introduce a global
  store.

## Required RED/GREEN Tests

Use public `App` behavior and deterministic deferred promises.

1. **Delayed open success loses to later B selection**
   - start Open Project;
   - allow picker to return a path, but keep open/register pending;
   - start/select project B;
   - resolve B and prove B is active;
   - resolve the older open/register request;
   - prove no final `selectProject(opened.id)` occurs, B remains active, no
     stale error appears, and resource list/recents/preview remain B-scoped.
2. **Delayed open failure loses to later B selection**
   - start Open Project with a deferred picker or open/register;
   - start/select B and resolve it;
   - reject the older open path with a recognizable error;
   - prove B remains active, the stale error banner is absent, and B resources
     remain usable.
3. Preserve existing open/register success, cancellation, direct-selection
   last-request-wins, and resource integration tests.

Record exact RED evidence before production changes and GREEN evidence after.

## Required Gates

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/project-workbench.test.tsx tests/resource-workbench-integration.test.tsx
pnpm --filter @pillow/desktop exec vitest run tests/resource-pane.test.tsx tests/resource-preview.test.tsx tests/resource-gateway.test.ts tests/conversation-workbench.test.tsx
pnpm --filter @pillow/desktop typecheck
pnpm --filter @pillow/desktop build:ui
git diff --check
```

Append changed files, RED/GREEN output, staging count, residual process count,
and remaining risk to the existing C2-D report. End with a clear completion
message. The controller will independently inspect the code and rerun all gates;
worker completion is not acceptance.
