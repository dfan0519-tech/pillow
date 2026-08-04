# Wave C2 Task 3 — Agent Activity Review Fix 2 Brief

**Date**: 2026-07-31
**Parent brief**:
`docs/execution/2026-07-31-wave-c2-agent-activity-timeline-brief.md`
**Prior fix brief**:
`docs/execution/2026-07-31-wave-c2-agent-activity-review-fix-brief.md`
**Parent report**:
`docs/execution/2026-07-31-wave-c2-agent-activity-timeline-report.md`
**Controller session**: `<keeper-session>`
**Commit policy**: do not stage or commit

## Confirmed Remaining Important Findings

Codex2 and controller independently confirmed two remaining Important issues:

1. `recordKey = identity::index` changes when an activity array is reordered or
   receives an earlier item. The timeline's single-record mount is safe, but
   the accepted disclosure component's array contract is not.
2. `artifact.resource.resourceId` and `tool.receipt.receiptId` are discarded
   before projection. A label/title that echoes either ID cannot be sanitized.

## Exclusive Write Whitelist

Modify only:

- `apps/desktop/src/features/conversations/agentActivityFromTimeline.ts`
- `apps/desktop/src/features/conversations/AgentActivityDisclosure.tsx`
- `apps/desktop/src/features/conversations/agentActivityTypes.ts`
- `apps/desktop/src/features/conversations/agentActivityProjection.ts`
- `apps/desktop/tests/agent-activity-timeline-integration.test.tsx`
- `docs/execution/2026-07-31-wave-c2-agent-activity-timeline-report.md`

Everything else is read-only. Preserve the complete dirty worktree. Do not
reset, restore, stage, commit, or write into legacy Keeper.

## Required Fixes

### Stable record identity under array reorder

- Add an optional internal, non-DOM record identity to
  `AgentActivityRecord`.
- The real timeline mapper must populate it from the real run identity.
- Projection must prefer that identity, then existing real run/session
  identity. Do not append current array index when a stable identity exists.
- Array index is allowed only as a legacy fallback for records that provide no
  stable identity at all.
- The identity must never enter text, attributes, titles, diagnostics, or test
  IDs.
- Add a regression with two distinct run records: open one disclosure, rerender
  the array in reverse order, and prove the same run's disclosure remains open.

### Complete internal-ID sanitization carrier

- Add an optional internal-only list of forbidden display IDs to
  `AgentActivityRecord`.
- The real mapper must populate it with every actual internal ID available in
  `RunActivityView`, including:
  - run ID;
  - plan-step IDs;
  - tool-call IDs;
  - receipt IDs;
  - tool resource IDs;
  - sub-agent IDs;
  - run artifact IDs;
  - embedded artifact resource IDs.
- Projection must merge that list into its existing forbidden-ID set and must
  never render or expose the carrier.
- Sanitize a fallback artifact path leaf as a display label too; if it itself
  echoes a forbidden ID, use neutral artifact chrome while leaving the
  callback path unchanged.
- Add adversarial regressions for:
  1. a receipt/resource label echoing its receipt ID;
  2. a run artifact embedded-resource label echoing the embedded resource ID
     while the artifact ID differs.
- Assert those IDs are absent from visible text and DOM attributes, while the
  portable callback path and exact resource resolution remain correct.

## Required Gates

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/agent-activity-timeline-integration.test.tsx
pnpm --filter @pillow/desktop exec vitest run tests/agent-activity-disclosure.test.tsx tests/agent-activity-timeline-integration.test.tsx tests/conversation-workbench.test.tsx
pnpm --filter @pillow/desktop typecheck
git diff --check
git diff --cached --name-only
```

Append exact changes, test counts, staging count, residual risks, and a
completion message to the parent report. End with `DONE`,
`DONE_WITH_CONCERNS`, `BLOCKED`, or `NEEDS_CONTEXT`. Worker completion is not
controller acceptance.
