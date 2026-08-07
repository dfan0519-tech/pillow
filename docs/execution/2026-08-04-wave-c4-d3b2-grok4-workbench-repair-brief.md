# C4 D3B2 Grok4 repair brief — approval availability and terminal refresh

Work only in `<repo>`. Preserve every unrelated dirty change. Do not
stage, commit, reset, restore, clean, package, publish, query Keeper/Vault for
project context, or spawn subagents. Keeper boot/completion calls are record
only; this brief is the complete task authority.

## Root causes already established by controller review

1. `applyListedApprovals` marks every pending write as
   `decisionAvailable: true`, even when the native list supplied no opaque
   `approvalGrant`. `handleWriteDecide` cannot decide such a proposal, so an
   older unavailable write can win arbitration and hide an actionable process
   proposal.
2. The refresh-token effect calls `clearApprovalScope()`, which always clears
   `terminalOutputTarget`. A process lifecycle event therefore closes the open
   terminal pane before `ResourcePane` can re-pull the updated bounded output.

## Objective

Make the smallest source-level repair while retaining the accepted one-slot,
generation-guarded architecture:

- a write candidate is decision-available only when its current listed view
  carries an opaque grant; never place the grant in React state, DOM, logs, or
  process calls;
- process event token changes within the same project/conversation preserve an
  open terminal target and cause the existing pane to re-pull output;
- project or conversation identity changes still synchronously prevent the
  foreign terminal target from painting or resurfacing;
- ordinary pane close still clears the target and never cancels the process.

## TDD evidence required

Add regression tests first and run them before editing product code. Record the
RED failure in the report. The tests must prove:

1. an older pending write without `approvalGrant` cannot hide a newer
   actionable process proposal;
2. after opening Terminal output, changing only
   `execProposalRefreshToken` keeps the pane open, calls `getExecOutput` again,
   and paints the refreshed host-owned chunks;
3. switching conversation or project still prevents the old output target and
   stale promise from painting.

Then implement the minimal fix and run the same tests GREEN.

## Exact write allowlist

- `apps/desktop/src/features/workbench/Workbench.tsx`
- `apps/desktop/tests/c4-workbench-process-integration.test.tsx`
- `docs/execution/2026-08-04-wave-c4-d3b2-grok4-workbench-repair-report.md`

No other file may change.

## Required commands

1. `pnpm --filter @pillow/desktop exec vitest run tests/c4-workbench-process-integration.test.tsx`
2. `pnpm --filter @pillow/desktop exec vitest run tests/c4-workbench-process-integration.test.tsx tests/c4-execution-primitives.test.tsx`
3. `pnpm --filter @pillow/desktop typecheck`
4. `git diff --check -- apps/desktop/src/features/workbench/Workbench.tsx apps/desktop/tests/c4-workbench-process-integration.test.tsx docs/execution/2026-08-04-wave-c4-d3b2-grok4-workbench-repair-report.md`

Write the report with exact changed files, RED and GREEN outputs, residual
risks, and `COMPLETION: complete|blocked`. Controller acceptance is separate.
