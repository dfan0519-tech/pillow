# Wave C2-D — Stale Picker Registration Fix Brief

**Date**: 2026-07-31
**Controller session**: `<keeper-session>`
**Baseline HEAD**: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
**Source**: Codex2 final review; controller confirmed Important
**Commit policy**: do not stage or commit

## Finding

`openProject()` captures a project-activation generation before
`pickDirectory()`, but after a delayed picker returns a non-empty path it calls
`openOrRegisterProject(path)` without first proving that generation is still
current.

An older Open Project action can therefore register project metadata after a
newer project B selection has won. Preventing only the final activation is
insufficient because registration is itself a persistent mutation.

## Exclusive Write Whitelist

Modify only:

- `apps/desktop/src/App.tsx`
- `apps/desktop/tests/project-workbench.test.tsx`
- `apps/desktop/tests/resource-workbench-integration.test.tsx`
- `docs/execution/2026-07-31-wave-c2d-resource-workbench-integration-report.md`

All other paths are read-only. Preserve the dirty worktree. Do not stage,
commit, reset, restore, or edit `<workspace>\VaultSystem\keeper`.

## Required Fix

Immediately after a non-empty picker result and before
`openOrRegisterProject(path)`, return silently if the captured generation is no
longer current.

Preserve the separate post-open/register generation check because a request
can become stale while registration is already in flight. Preserve current
picker cancellation, error ownership, busy ownership, direct-selection guards,
conversation clearing, and resource-scope protections.

## Required RED/GREEN Tests

Use public `App` behavior and a deferred `pickDirectory()`:

1. start Open Project and keep the picker pending;
2. start/select project B and resolve B;
3. prove B is active;
4. resolve the older picker with a valid path;
5. prove `openOrRegisterProject` was never called;
6. prove B remains active, no stale error appears, and busy ownership allows
   the B-scoped resource panel to remain usable;
7. in the resource integration suite, prove list/recents/preview remain
   B-scoped only.

Record exact RED output before production changes. Preserve all eight existing
direct-selection and in-flight-open race tests.

## Required Gates

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/project-workbench.test.tsx tests/resource-workbench-integration.test.tsx
pnpm --filter @pillow/desktop exec vitest run tests/resource-pane.test.tsx tests/resource-preview.test.tsx tests/resource-gateway.test.ts tests/conversation-workbench.test.tsx
pnpm --filter @pillow/desktop typecheck
pnpm --filter @pillow/desktop build:ui
git diff --check
```

Append RED/GREEN evidence, changed files, staging count, process cleanup, and
remaining risk to the C2-D report. Worker completion is not controller
acceptance.
