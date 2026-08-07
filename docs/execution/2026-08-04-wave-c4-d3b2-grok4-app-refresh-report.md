# C4 D3B2 Grok4 report — App process refresh composition

## Summary

Wired `ExecGateway` through renderer composition and added one event-driven
`execProposalRefreshToken` that bumps only for the four accepted process
lifecycle kinds when the event belongs to the selected conversation under the
active project. Existing write refresh is unchanged and isolated from the
process token. No polling, process cache, authority/grant/output, native path,
Workbench implementation, or styling changes.

## Changed files

| File | Change |
|------|--------|
| `apps/desktop/src/App.tsx` | Optional `execGateway` prop; `execProposalRefreshToken` state; subscribe bumps for `process_proposal_requested`, `process_proposal_decided`, `process_started`, `process_terminal_outcome` when selected + `conversationBelongsToProject`; pass both props to Workbench; preserve write token path |
| `apps/desktop/src/main.tsx` | Construct one `TauriExecGateway` and pass `execGateway` into `App` |
| `apps/desktop/tests/c4-app-process-refresh.test.tsx` | Focused composition tests (gateway pass-through, four kinds, foreign conversation, foreign project, write regression / token isolation) |
| `docs/execution/2026-08-04-wave-c4-d3b2-grok4-app-refresh-report.md` | This report |

No other files were modified.

## Behavior

- **Refresh token:** single `execProposalRefreshToken` (number, starts at 0).
- **Bump kinds (exactly four):**
  1. `process_proposal_requested`
  2. `process_proposal_decided`
  3. `process_started`
  4. `process_terminal_outcome`
- **Ownership gate:** event `conversationId` must equal the currently selected
  conversation **and** `conversationBelongsToProject` for the active project.
- **Write path:** still bumps only on `write_proposal_requested` /
  `write_proposal_resolved` under the same ownership gate; process events do
  not touch `writeProposalRefreshToken` and write events do not touch
  `execProposalRefreshToken`.
- **Composition:** `main.tsx` → `App` → Workbench props
  `execGateway?: ExecGateway` and `execProposalRefreshToken?: number`.

## Commands and results

```text
pnpm --filter @pillow/desktop exec vitest run tests/c4-app-process-refresh.test.tsx
```

| Metric | Result |
|--------|--------|
| Test files | 1 passed |
| Tests | **5 passed / 5** |
| Duration | ~3.6s |

Cases covered:

1. Passes `execGateway`; tokens start at 0  
2. All four process kinds each bump exec token once; write token unchanged  
3. Foreign conversation (same project) does not bump  
4. Foreign project conversation does not bump  
5. Write requested/resolved still bump write token only; process bumps exec only  

```text
pnpm --filter @pillow/desktop typecheck
```

| Result | Detail |
|--------|--------|
| **Failed (expected residual)** | `src/App.tsx`: Property `execGateway` does not exist on `WorkbenchProps` (and the same for `execProposalRefreshToken` once that prop is reached) |

## Residual dependency

Workbench (`apps/desktop/src/features/workbench/Workbench.tsx`) does **not** yet
declare:

```ts
execGateway?: ExecGateway;
execProposalRefreshToken?: number;
```

This slice must not edit Workbench (non-overlapping D3B2 allowlist). After the
controller merges the Workbench mount slice that accepts these exact prop names,
desktop typecheck should clear without further App/main changes.

## Out of scope (intentionally not done)

- Polling or interval timers  
- Process proposal state cache in App  
- Grant / output / native path handling  
- Workbench list/decide/review or styling  
- Stage, commit, reset, restore, clean, package, publish  

## COMPLETION: complete
