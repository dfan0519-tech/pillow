# Wave C2 — Agent Activity Dependent Tests Migration Brief

**Date**: 2026-07-31
**Controller session**: `<keeper-session>`
**Parent task**:
`docs/execution/2026-07-31-wave-c2-agent-activity-timeline-brief.md`
**Commit policy**: do not stage or commit

## Confirmed Problem

The Task 3 implementation correctly replaced the timeline's legacy
`RunActivity` mount with `AgentActivityDisclosure`, but three dependent suites
were outside the original whitelist and still query legacy `run-activity`
test IDs.

The controller independently reproduced **7 failures** across:

- `conversation-gateway-stream.test.tsx`
- `conversation-fix-pass2.test.tsx`
- `resource-workbench-integration.test.tsx`

The rendered DOM contained the new `agent-activity-disclosure`; failures were
old selectors and old visible-copy expectations.

## Exclusive Write Whitelist

Modify only:

- `apps/desktop/tests/conversation-gateway-stream.test.tsx`
- `apps/desktop/tests/conversation-fix-pass2.test.tsx`
- `apps/desktop/tests/resource-workbench-integration.test.tsx`
- `docs/execution/2026-07-31-wave-c2-agent-activity-timeline-report.md`

All production files and all other paths are read-only. Preserve the dirty
worktree. Do not stage, commit, reset, restore, or edit legacy Keeper.

## Required Migration

- Replace legacy `run-activity`, `run-status`, plan/tool/sub-agent selectors
  with the new public Agent activity disclosure surface.
- Keep each test's behavioral purpose unchanged:
  - real streamed plan/tool/sub-agent data is visible after progressive
    disclosure;
  - completed/cancelled state is visible and not confused;
  - conversation artifact callbacks still open the accepted resource pane;
  - close/reopen preview resurrection protections remain tested;
  - resourceGateway omission still preserves the conversation-reference
    fallback.
- Expand the outer disclosure and nested artifact/output disclosure through
  semantic `<summary>` interaction or narrowly scoped DOM state before
  asserting nested content. Do not assert hidden content as visible.
- Prefer role, accessible name, status text, and accepted
  `agent-activity-*` test IDs. Do not add compatibility IDs to production code
  and do not weaken assertions.
- Do not rewrite the tests into snapshots or mock-only assertions.

## Required Gates

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/conversation-gateway-stream.test.tsx tests/conversation-fix-pass2.test.tsx tests/resource-workbench-integration.test.tsx
pnpm --filter @pillow/desktop exec vitest run tests/agent-activity-disclosure.test.tsx tests/agent-activity-timeline-integration.test.tsx tests/conversation-workbench.test.tsx
pnpm --filter @pillow/desktop typecheck
git diff --check
```

Append exact changes, GREEN evidence, staging count, process cleanup, and
remaining concerns to the parent report. End with `DONE`,
`DONE_WITH_CONCERNS`, `BLOCKED`, or `NEEDS_CONTEXT`. Worker completion is not
controller acceptance.
