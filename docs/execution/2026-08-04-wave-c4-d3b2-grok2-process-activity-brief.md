# C4 D3B2 Grok2 brief — compact conversation-owned command activity

Work only in `<repo>`. Preserve every unrelated dirty change. Do not
stage, commit, reset, restore, clean, package, publish, query Keeper/Vault for
project context, or spawn subagents. Keeper boot/completion calls are record
only; this brief is the complete task authority.

## Objective

Replace the current four generic process lifecycle notices with one compact,
conversation-owned command activity row per real proposal. Keep the existing
conversation timeline as the visual centre and use progressive disclosure:

`summary row -> command/lifecycle details -> 完整输出 in existing resource pane`.

Do not create a dashboard, permanent task-card stack, polling, another process
cache, or another output renderer.

## Accepted inputs and authority

- Source only from strict normalized `RuntimeEventDto` process events already
  carried by the owning `ConversationTimelineView.events`.
- Group only events matching both the owning conversation and run.
- `proposalId` is an internal grouping/callback key only. Never paint it in
  text, DOM attributes, titles, test IDs, logs, or errors.
- Output remains pull-on-demand through the accepted `ExecGateway` and existing
  `Terminal output` pane. Do not put transcript bytes or digests in timeline
  state. The optional inline tail is intentionally omitted in this slice.

## Presentation contract

- Add pure `processActivityModel.ts` that groups durable lifecycle events into
  one ordered model per proposal and derives fixed safe status copy.
- Summary shows safe program label plus current lifecycle status. Completed
  success stays collapsed and concise.
- Expanded details may show literal argv tokens, decision/state, exit code,
  duration, captured bytes, truncated/lossy notices. Redact native absolute or
  UNC path-looking argv tokens; portable relative arguments remain literal.
- Never render project roots, portable cwd, raw proposal/run/session/event IDs,
  grants, environment values, stdout/stderr digests, raw host errors, or debug
  metadata.
- Proposal/decision/start/terminal order comes from durable sequence, while the
  UI groups them into one row rather than four permanent notices.
- Show `完整输出` only when the process has started or reached a terminal
  outcome. Clicking it passes the internal proposal identity plus safe program
  label/argv to Workbench, which opens the already accepted Terminal output
  target in `ResourcePane`.
- Project/conversation switching must not paint or reopen foreign activity.

## Existing behavior to replace

`agentActivityFromTimeline.ts` currently maps the four process events into
`processActivity`, and `agentActivityProjection.ts` turns them into generic
notice paragraphs. Remove that duplicate notice path once the grouped command
row is mounted. Preserve write outcomes and all non-process agent activity.

## Exact write allowlist

- Create `apps/desktop/src/features/execution/processActivityModel.ts`
- Modify `apps/desktop/src/features/conversations/ConversationTimeline.tsx`
- Modify `apps/desktop/src/features/workbench/Workbench.tsx`
- Modify `apps/desktop/src/features/conversations/agentActivityFromTimeline.ts`
- Modify `apps/desktop/src/features/conversations/agentActivityProjection.ts`
- Modify `apps/desktop/src/features/conversations/agentActivityTypes.ts`
- Modify `apps/desktop/tests/agent-activity-timeline-integration.test.tsx` only
  to replace prior generic-process-notice assertions
- Create `apps/desktop/tests/c4-process-activity-integration.test.tsx`
- Create `docs/execution/2026-08-04-wave-c4-d3b2-grok2-process-activity-report.md`

No other file may change. Reuse existing activity CSS classes; do not edit CSS.

## Required TDD coverage

Write focused tests first and record RED before product edits. Prove:

1. proposal/decision/start/terminal events group into one row in durable order;
2. safe program/argv and terminal metadata appear only after disclosure;
3. successful commands remain concise/collapsed;
4. `完整输出` routes the exact internal proposal to the existing Terminal pane;
5. raw IDs, native paths, cwd, grants, environment, digests, transcript and
   debug fields never appear in the DOM;
6. foreign conversation/run events do not paint;
7. the old four generic notice paragraphs are gone and non-process agent
   activity/write outcomes remain unchanged.

## Required commands

1. `pnpm --filter @pillow/desktop exec vitest run tests/c4-process-activity-integration.test.tsx tests/agent-activity-timeline-integration.test.tsx tests/c4-workbench-process-integration.test.tsx`
2. `pnpm --filter @pillow/desktop typecheck`
3. `git diff --check -- <all allowlisted code/test/report files>`

Write the report with exact changed files, RED/GREEN outputs, pass counts,
residual risks, and `COMPLETION: complete|blocked`. Controller acceptance is
separate.
