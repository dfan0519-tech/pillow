# Wave C2 Task 3 — Agent Activity Review Fix Brief

**Date**: 2026-07-31
**Parent brief**:
`docs/execution/2026-07-31-wave-c2-agent-activity-timeline-brief.md`
**Parent report**:
`docs/execution/2026-07-31-wave-c2-agent-activity-timeline-report.md`
**Controller session**: `<keeper-session>`
**Commit policy**: do not stage or commit

## Review Result

Codex2 independently returned:

- Spec Compliance: `不符合`
- Code Quality: `Needs fixes`
- Critical: 0
- Important: 6

This is a narrow review-fix pass. Do not add product features or change the
accepted C2 resource/workbench contracts.

## Exclusive Write Whitelist

Modify only:

- `apps/desktop/src/features/conversations/agentActivityFromTimeline.ts`
- `apps/desktop/src/features/conversations/ConversationTimeline.tsx`
- `apps/desktop/src/features/conversations/AgentActivityDisclosure.tsx`
- `apps/desktop/src/features/conversations/agentActivityTypes.ts`
- `apps/desktop/src/features/conversations/agentActivityProjection.ts`
- `apps/desktop/tests/agent-activity-timeline-integration.test.tsx`
- `docs/execution/2026-07-31-wave-c2-agent-activity-timeline-report.md`

Everything else is read-only. Preserve the complete dirty worktree. Do not
reset, restore, stage, commit, or write into legacy Keeper.

## Required Fixes

### 1. Stable disclosure identity

- Render each projected record with `key={projection.recordKey}`.
- The key remains React-internal and must not enter text, attributes, titles,
  diagnostics, or test IDs.
- Add a regression that opens a disclosure, rerenders the same run with a new
  status/current step, and proves the disclosure remains open.

### 2. Exact artifact/resource identity

The visible artifact and the resource opened by its button must be the same
source object, even when paths collide.

- Preserve all portable tool resources in tool/resource order, followed by all
  portable run artifacts in run order. Do not silently deduplicate different
  sources solely because their paths match.
- Add a non-DOM internal source token to each mapped/projected artifact.
- Extend the disclosure callback compatibly to
  `(relativePath: string, sourceToken?: string)`. Existing one-argument
  consumers must remain valid.
- Pass the token only through the in-memory click callback. It must never be
  written to DOM text or attributes.
- Resolve by source token first:
  - a tool-resource token returns only that exact matching tool resource;
  - a run-artifact token returns that exact artifact's embedded resource only
    when the embedded resource has the same portable path;
  - otherwise return the minimal reference derived from the real run
    artifact path.
- Keep a safe path-only fallback for legacy one-argument callbacks/records.
- If `artifact.resource.path` differs from `artifact.path`, do not borrow its
  label, content, or reference. Display the artifact path leaf and open a
  minimal reference for `artifact.path`.

Add regressions for:

1. path-mismatched embedded resource: display/open artifact A, never resource
   B;
2. a tool resource and run artifact with the same portable path but different
   labels/previews: each button opens its own exact reference.

### 3. Real ownership and chronology assertions

- Replace the current foreign-ownership no-op with a real disclosure-level
  render containing owned and foreign records. Assert owned content renders
  and foreign content/IDs do not.
- In the chronology test, expand both activity disclosures and assert the
  actual DOM order: early activity < middle message < late activity.
- Do not keep unused index variables or assertion-free render/unmount blocks.

### 4. Defensive ID non-disclosure

- Display fields must not echo known internal run, step, tool-call, sub-agent,
  artifact, receipt, or resource IDs merely because an upstream
  label/title/progress field repeats such an ID.
- Sanitize exact or whole-token occurrences in visible labels/titles/progress,
  falling back to neutral UI chrome or the safe artifact path leaf.
- Do not mutate the portable path passed to the resource callback and do not
  redact unrelated ordinary user text.
- Add an adversarial fixture where label/title fields repeat associated raw
  IDs; assert those IDs are absent from rendered text and attributes.

## Required Gates

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/agent-activity-timeline-integration.test.tsx
pnpm --filter @pillow/desktop exec vitest run tests/agent-activity-disclosure.test.tsx tests/agent-activity-timeline-integration.test.tsx tests/conversation-workbench.test.tsx
pnpm --filter @pillow/desktop exec vitest run tests/conversation-gateway-stream.test.tsx tests/conversation-fix-pass2.test.tsx tests/resource-workbench-integration.test.tsx
pnpm --filter @pillow/desktop typecheck
pnpm --filter @pillow/desktop build:ui
git diff --check
git diff --cached --name-only
```

Append exact changes, test counts, staging count, residual risks, and a
completion message to the parent report. End with `DONE`,
`DONE_WITH_CONCERNS`, `BLOCKED`, or `NEEDS_CONTEXT`. Worker completion is not
controller acceptance.
