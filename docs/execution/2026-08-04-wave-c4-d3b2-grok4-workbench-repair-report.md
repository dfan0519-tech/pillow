# C4 D3B2 Grok4 repair report — approval availability and terminal refresh

## Scope

Source-level repair only, under the exact allowlist:

| File | Role |
| --- | --- |
| `apps/desktop/src/features/workbench/Workbench.tsx` | Product fix |
| `apps/desktop/tests/c4-workbench-process-integration.test.tsx` | Regression tests (TDD RED → GREEN) |
| `docs/execution/2026-08-04-wave-c4-d3b2-grok4-workbench-repair-report.md` | This report |

No other files modified. No stage/commit/package/publish. No Keeper/Vault context pack. No subagents.

## Root causes (confirmed)

1. **Grant-less write wins arbitration.** `applyListedApprovals` always set
   `decisionAvailable: true` for pending writes, even when the native list had
   no opaque `approvalGrant`. Arbitration therefore could surface an older
   non-decidable write and hide a newer host-available process proposal.
2. **Refresh token closes terminal.** The shared re-list effect always called
   `clearApprovalScope()`, which always cleared `terminalOutputTarget`. A
   same-scope `execProposalRefreshToken` bump therefore closed the open
   terminal pane before `ResourcePane` could re-pull host chunks.

## Product fix (minimal)

### 1. Write decision availability = grant present on current list

In `applyListedApprovals`, write candidates now set:

```ts
decisionAvailable: Boolean(item.approvalGrant)
```

The opaque grant still lives only in `grantByProposalRef` (never React state,
DOM, logs, or process decide calls). Process candidates continue to use the
host `item.decisionAvailable` bit.

### 2. Preserve terminal on same-scope token refresh

`clearApprovalScope` accepts `{ clearTerminal?: boolean }` (default true).

The re-list effect now:

- bumps the generation guard as before;
- detects project/conversation **identity** change;
- clears terminal only when identity changed or scope is empty;
- keeps `terminalOutputTarget` when only refresh tokens / gateways change so
  `ResourcePane` re-pulls via existing `execProposalRefreshToken` dependency.

Ordinary pane close still clears the target and never cancels a process.
Conversation/project switches still drop the foreign target so it cannot paint
or resurface.

## TDD evidence

### Regression tests added

1. `does not let an older grant-less write hide a newer actionable process`
2. `keeps terminal open and re-pulls when only execProposalRefreshToken changes`
3. `suppresses stale terminal output after project switch` (conversation case
   already present; project case added for identity coverage)

### RED (tests first, product code untouched)

```text
pnpm --filter @pillow/desktop exec vitest run tests/c4-workbench-process-integration.test.tsx

 ❯ tests/c4-workbench-process-integration.test.tsx (22 tests | 2 failed) 2535ms
   × does not let an older grant-less write hide a newer actionable process
     → Unable to find an element by: [data-testid="command-approval-bar"]
     (DOM showed write-approval-bar instead)
   × keeps terminal open and re-pulls when only execProposalRefreshToken changes
     → Unable to find an element by: [data-testid="terminal-output-pane"]
     (resource pane stayed open; terminal mode cleared)

 Test Files  1 failed (1)
      Tests  2 failed | 20 passed (22)
```

### GREEN (after product fix)

```text
pnpm --filter @pillow/desktop exec vitest run tests/c4-workbench-process-integration.test.tsx
 ✓ tests/c4-workbench-process-integration.test.tsx (22 tests) 1620ms
 Test Files  1 passed (1)
      Tests  22 passed (22)

pnpm --filter @pillow/desktop exec vitest run tests/c4-workbench-process-integration.test.tsx tests/c4-execution-primitives.test.tsx
 ✓ tests/c4-execution-primitives.test.tsx (25 tests) 786ms
 ✓ tests/c4-workbench-process-integration.test.tsx (22 tests) 1696ms
 Test Files  2 passed (2)
      Tests  47 passed (47)

pnpm --filter @pillow/desktop typecheck
 $ tsc --noEmit
 EXIT=0

git diff --check -- apps/desktop/src/features/workbench/Workbench.tsx apps/desktop/tests/c4-workbench-process-integration.test.tsx docs/execution/2026-08-04-wave-c4-d3b2-grok4-workbench-repair-report.md
 EXIT=0
```

## Residual risks

- Grant-less pending writes no longer surface the write bar at all (they are not
  decision-available). Review-only surfacing without a grant is intentionally
  out of this repair; native must supply a grant for a write to enter the one
  visible bar.
- Same-scope write-token refresh still clears `writeDiffTarget` (only terminal
  preservation was required for process lifecycle re-pull).
- Real Tauri dual-domain E2E remains controller-owned.
- Controller acceptance is separate from this worker completion.

COMPLETION: complete
