# Wave C2 — Real Conversation Agent Activity Timeline Brief

**Date**: 2026-07-31
**Controller session**: `<keeper-session>`
**Baseline HEAD**: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
**Plan task**: Task 3 of
`docs/superpowers/plans/2026-07-31-c2-read-only-workbench-completion.md`
**Commit policy**: do not stage or commit

## Objective

Replace the timeline's mounted `RunActivity` rendering site with the accepted
`AgentActivityDisclosure`, driven only by the owning conversation's real
`RunActivityView`. Keep the conversation as the visual center and preserve
chronological timeline behavior, progressive disclosure, portable resource
opening, bounded outputs, and project/conversation isolation.

This task must not create a second activity store, poll global sessions,
fabricate roles/progress/tests, expose raw IDs, or broaden C2 capabilities.

## Current Accepted Inputs

- `ConversationTimelineView.activitiesByRunId` is the real durable per-run
  projection already owned by one `timeline.conversationId`.
- `RunActivityView` contains real plan steps, tools, sub-agents, artifacts,
  bounded output/truncation metadata, and omission counters.
- `AgentActivityDisclosure` is an accepted, closed-by-default progressive
  disclosure component.
- `Workbench` already accepts `ConversationTimeline.onOpenResource` and opens
  portable paths in the accepted C2-D contextual resource pane.
- The old `RunActivity.tsx` remains read-only and may remain in the tree for
  focused legacy regression tests; remove only its timeline render-site use.

## Exclusive Write Whitelist

Modify or create only:

- `apps/desktop/src/features/conversations/agentActivityFromTimeline.ts` (new)
- `apps/desktop/src/features/conversations/ConversationTimeline.tsx`
- `apps/desktop/src/features/conversations/AgentActivityDisclosure.tsx`
- `apps/desktop/src/features/conversations/agentActivityTypes.ts`
- `apps/desktop/src/features/conversations/agentActivityProjection.ts`
- `apps/desktop/tests/agent-activity-timeline-integration.test.tsx` (new)
- `apps/desktop/tests/conversation-workbench.test.tsx`
- `docs/execution/2026-07-31-wave-c2-agent-activity-timeline-report.md` (new)

Everything else is read-only, including `RunActivity.tsx`,
`conversationTypes.ts`, reducers, App, Workbench, resource code, Rust/Tauri,
provider/runtime code, package files, and `legacy Keeper (out of tree)`.
Preserve the complete existing dirty worktree. Do not reset, restore, stage, or
commit.

## Required Mapper Contract

```ts
export function agentActivityFromRun(
  conversationId: string,
  activity: RunActivityView,
): AgentActivityRecord;
```

The mapper is pure and creates no state.

- Copy `conversationId` from the owning timeline.
- Keep `runId`, plan step IDs, tool-call IDs, sub-agent IDs, artifact IDs, and
  resource IDs only as internal keys. They must never become visible text,
  attributes, labels, titles, diagnostics, or test IDs.
- Copy real status values. Add safe labels for real `cancelling` and
  `interrupted` states rather than exposing raw English tokens.
- Derive `currentStep` only from an actual active plan step. If none exists,
  omit it.
- Copy plan steps in order.
- Copy tools in order with actual title/name, status, and bounded output.
  Failed/cancelled tool output is error material and must use the accepted
  opaque-error path; completed/running output remains real bounded output.
- Preserve tool output truncation counts as visible, human-readable summaries.
- Copy sub-agents in order using only actual title, status, and progress. Do
  not invent a role that is absent from `RunActivityView`.
- Project portable artifacts from tool resources in tool/resource order,
  followed by run artifacts in run order. Use actual labels/paths. Drop
  absolute, backslash, URL, empty-segment, and traversal paths through the
  accepted portable-path projection.
- Preserve all existing run-level truncation/omission counts as compact
  summaries: assistant, reasoning, plan steps, tools, sub-agents, artifacts,
  resources, and resource-preview characters.
- `RunActivityView` has no structured test-result DTO. Leave `testResults`
  empty; never infer test success from tool names or output.
- Do not expose reasoning text or invent chain-of-thought content. Only its
  existing truncation count may appear as a summary.

Extend the accepted Agent activity types/projection minimally so:

- each projected record and item has a stable internal key;
- run-level notices and per-tool output-truncation notices can be rendered;
- these keys never enter DOM attributes or visible copy;
- existing C2-C2 behavior and nine focused tests remain compatible.

## Timeline Mount Contract

- Keep chronological item construction, timestamps, pagination, visible-range
  freezing, and follow-bottom behavior unchanged.
- At the existing activity render site, create exactly one
  `AgentActivityRecord` from `timeline.conversationId` and that
  `RunActivityView`, then render `AgentActivityDisclosure`.
- Do not render both `RunActivity` and `AgentActivityDisclosure`.
- Preserve `ConversationTimeline`'s public `onOpenResource` contract. Adapt
  the disclosure's portable-path callback back to the matching actual
  `ResourceReferenceView` from the run. If an artifact has a real portable
  path but no embedded resource, derive only the minimal reference necessary
  to open that real path; do not fabricate content or test data.
- Foreign-conversation records render nothing. Project switching must not
  retain the prior conversation's activity.

## Required TDD Evidence

Write the new integration test first and record RED before production changes.
Use actual `RunActivityView` fixtures and public DOM behavior.

Cover at minimum:

1. timeline renders `AgentActivityDisclosure`, not the old `RunActivity`;
2. foreign conversation record is filtered;
3. plan/tool/sub-agent order and actual statuses/progress are preserved;
4. `runId`, tool-call ID, sub-agent ID, artifact/resource IDs never appear in
   rendered text or DOM attributes;
5. live/terminal/failure/interrupted/cancelling states remain accessible;
6. tool and run artifacts open only portable paths through the existing
   `onOpenResource` callback;
7. absolute/traversal/URL paths never invoke the callback;
8. output, output-truncation, and all run omission summaries remain visible
   after disclosure expansion;
9. missing structured tests/roles render no invented test or role content;
10. chronological and project/conversation isolation regressions remain green.

The RED run must fail because the mapper/mount does not yet exist or the old
`RunActivity` is still mounted. Record the exact failure.

## Required Gates

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/agent-activity-timeline-integration.test.tsx
pnpm --filter @pillow/desktop exec vitest run tests/agent-activity-disclosure.test.tsx tests/agent-activity-timeline-integration.test.tsx tests/conversation-workbench.test.tsx
pnpm --filter @pillow/desktop typecheck
pnpm --filter @pillow/desktop build:ui
git diff --check
```

Write the completion report with:

- status (`DONE`, `DONE_WITH_CONCERNS`, `BLOCKED`, or `NEEDS_CONTEXT`);
- exact changed files;
- RED/GREEN evidence and counts;
- contract mapping table;
- provenance statement;
- self-review findings;
- staging count and residual process count;
- residual risks;
- a clear completion message.

Worker completion is not acceptance. The controller will inspect the complete
diff and personally rerun focused and regression gates before Codex2 review.
