# Wave C2 Task 3 — Duplicate-Key Test Fixture Cleanup

**Date**: 2026-07-31
**Controller session**: `<keeper-session>`
**Commit policy**: do not stage or commit

## Confirmed Problem

Production review is approved with no Critical or Important findings. One
legacy disclosure test constructs two distinct synthetic run records by calling
`baseRecord` twice without overriding its shared `runId`. React correctly emits
a duplicate-key warning even though the assertions pass. Real timeline records
are unique by `activitiesByRunId`.

## Exclusive Write Whitelist

Modify only:

- `apps/desktop/tests/agent-activity-disclosure.test.tsx`
- `docs/execution/2026-07-31-wave-c2-agent-activity-timeline-report.md`

All production files and all other paths are read-only. Preserve the dirty
worktree. Do not stage, commit, reset, or restore.

## Required Change

- In the one multi-record terminal failure/cancelled test, give the two
  synthetic logical runs distinct stable internal identities (prefer distinct
  `runId` values).
- Do not change product behavior, assertions, visible copy, or snapshot
  expectations.
- Do not suppress `console.error` and do not weaken the test.

## Required Gates

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/agent-activity-disclosure.test.tsx
pnpm --filter @pillow/desktop exec vitest run tests/agent-activity-disclosure.test.tsx tests/agent-activity-timeline-integration.test.tsx tests/conversation-workbench.test.tsx
git diff --check
git diff --cached --name-only
```

Append exact change, gate counts, staging count, and completion status to the
parent report. Worker completion is not controller acceptance.
