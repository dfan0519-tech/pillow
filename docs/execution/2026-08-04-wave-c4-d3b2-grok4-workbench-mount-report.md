# C4 D3B2 Grok4 — Workbench process governance mount report

## Delivered

Mounted the accepted C4 process seams into the conversation-centred Workbench
without a second dashboard:

- `Workbench` consumes `execGateway?: ExecGateway` and
  `execProposalRefreshToken?: number`.
- Write and process candidates are listed under one project/conversation
  generation guard and arbitrated with pure `selectVisibleApproval`, so exactly
  one write-or-process approval bar is visible.
- Process approve-once / reject call `ExecGateway.decideExecProposal` with
  conversation id, proposal id, and decision only — no grant — then re-list
  from native authority.
- `查看命令` opens a `Terminal output` mode inside the existing contextual
  `ResourcePane` via pull-on-demand `openExecOutput`.
- `ExecOutputView.liveRetained` is propagated into `TerminalOutputModel`; when
  false, `TerminalOutputView` shows an explicit durable-only notice and never
  invents transcript bytes from metadata.
- Stale list, decision, and output promises cannot paint foreign approval,
  output, or error state after project/conversation generation changes.
- Closing the resource pane clears local terminal presentation only and never
  cancels a process. Escape rejects only when the process approval bar owns
  focus (existing `CommandApprovalBar` contract).

## Changed files (exact allowlist)

| File | Change |
| --- | --- |
| `apps/desktop/src/features/workbench/Workbench.tsx` | Dual-domain list/decide arbitration; process bar; terminal target; generation guards |
| `apps/desktop/src/features/workbench/ResourcePane.tsx` | Terminal output mode; pull-on-event open; close does not cancel |
| `apps/desktop/src/features/execution/executionTypes.ts` | Required `liveRetained` on `TerminalOutputModel` |
| `apps/desktop/src/features/execution/TerminalOutputView.tsx` | Durable-only notice when `liveRetained=false` |
| `apps/desktop/tests/c4-workbench-process-integration.test.tsx` | New focused mount/integration suite |
| `apps/desktop/tests/c4-execution-primitives.test.tsx` | `liveRetained` presentation contract fixtures |
| `docs/execution/2026-08-04-wave-c4-d3b2-grok4-workbench-mount-report.md` | This report |

No CSS files required changes. No other files were modified.

## Focused verification

1. `pnpm --filter @pillow/desktop exec vitest run tests/c4-workbench-process-integration.test.tsx tests/c4-execution-primitives.test.tsx`
   - PASS: 2 files, **44 tests passed** (19 integration + 25 primitives), 0 failed.
2. `pnpm --filter @pillow/desktop typecheck`
   - PASS: `tsc --noEmit`.

### Integration coverage

- Write-only, process-only, simultaneous candidates, exact ties, unavailable /
  invalid-expiry candidates, never two bars
- Approve-once / reject with no grant and authoritative re-list
- Stale process list after conversation and project switch
- Stale terminal output after conversation switch
- Durable-only notice when `liveRetained=false`
- Close pane does not cancel / does not decide reject
- Escape rejects only when process bar owns focus
- Write path regression (grant decide + governed diff)
- No process list polling; event-token re-list

## Residual risks

- Process activity progressive disclosure (`CommandActivityRow` in the timeline)
  remains a later D3B2 task; this mount does not add permanent task cards.
- Real Tauri process-governance E2E is still controller-owned and not part of
  this worker slice.
- `decisionAvailable: true` is used for pending write candidates so review-only
  writes still surface; process candidates keep the host `decisionAvailable`
  bit. Controller product review should confirm that mapping if write grants
  are ever absent while state remains `pending_approval`.
- Terminal re-pull on `execProposalRefreshToken` reloads the open target only;
  there is no interval poll and no process cache outside Workbench/ResourcePane.

Controller acceptance remains separate.

COMPLETION: complete
