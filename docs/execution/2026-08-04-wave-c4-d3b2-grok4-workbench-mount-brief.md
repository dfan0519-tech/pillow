# C4 D3B2 Grok4 brief — Workbench process governance mount

Work only in `<repo>`. Preserve every unrelated dirty change. Do not
stage, commit, reset, restore, clean, package, publish, call Keeper, query or
ingest Vault material, or spawn subagents. The controller has already recorded
the worker session and exact Grok4 lease.

## Objective

Mount the accepted C4 process seams into the existing conversation-centred
Workbench without creating another dashboard:

- consume `execGateway?: ExecGateway` and `execProposalRefreshToken?: number`;
- keep exactly one visible write-or-process approval bar by using the accepted
  pure `selectVisibleApproval` arbitration contract;
- list and decide process proposals through `ExecGateway`, with native
  authoritative re-list after every decision and no renderer grant;
- open bounded process output as a `Terminal output` mode inside the existing
  contextual `ResourcePane`;
- propagate `ExecOutputView.liveRetained` into the terminal presentation model;
  when false, show an explicit durable-only/output-not-retained notice and do
  not reconstruct transcript bytes from metadata;
- use project/conversation generation guards so stale list, decision, and
  output promises cannot paint foreign approval, output, or error state;
- closing the pane must never cancel a process; Escape rejects only when the
  process approval bar itself owns focus.

Do not add polling, interval timers, a process cache outside the Workbench,
raw grants, native paths, runtime/session IDs, terminal execution, Git, new
global stores, permanent task cards, or debug metadata.

## Exact write allowlist

- `apps/desktop/src/features/workbench/Workbench.tsx`
- `apps/desktop/src/features/workbench/ResourcePane.tsx`
- `apps/desktop/src/features/execution/executionTypes.ts`
- `apps/desktop/src/features/execution/TerminalOutputView.tsx`
- `apps/desktop/src/styles/workbench.css` only if layout integration needs it
- `apps/desktop/src/styles/execution.css` only if mounted-state styling needs it
- `apps/desktop/tests/c4-workbench-process-integration.test.tsx`
- `apps/desktop/tests/c4-execution-primitives.test.tsx` only if the required
  `liveRetained` presentation contract needs fixture coverage
- `docs/execution/2026-08-04-wave-c4-d3b2-grok4-workbench-mount-report.md`

No other file may change.

## Required evidence

Focused tests must cover write-only, process-only, simultaneous candidates,
exact ties, unavailable/expired candidates, and never rendering two bars;
approve-once/reject with no grant and authoritative re-list; stale
project/conversation list and output results; durable-only notice; close does
not cancel; focused Escape; and the existing write path regression.

Run:

1. `pnpm --filter @pillow/desktop exec vitest run tests/c4-workbench-process-integration.test.tsx tests/c4-execution-primitives.test.tsx`
2. `pnpm --filter @pillow/desktop typecheck`

Write the required report with exact changed files, commands, pass counts, and
residual risks. Finish `COMPLETION: complete` only when the implementation,
new test, report, focused tests, and typecheck all exist/pass; otherwise finish
`COMPLETION: blocked`. Controller acceptance remains separate.
