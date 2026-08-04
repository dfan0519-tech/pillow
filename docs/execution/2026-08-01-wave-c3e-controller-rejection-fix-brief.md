# Wave C3-E Controller Rejection Fix Brief

Owner: Grok implementation worker

Controller session: `<keeper-session>`

Status: ACTIVE — the first C3-E delivery is not accepted.

## Verdict

REJECT pending the focused corrections below. Preserve the existing dirty
tree. Do not stage or commit. Renderer/tests/report only; no Rust/native edits.

## Findings To Close

### 1. Raw grants can outlive their live proposal

`Workbench.relistProposals` adds grants to `grantByProposalRef` without first
removing grants that the authoritative list no longer returns. A proposal
terminalized by another actor can therefore leave its raw grant resident until
a scope switch. The brief permits a grant only for the exact currently selected
live pending proposal.

After a same-scope list succeeds, atomically replace renderer grant memory:

- choose the oldest owned pending proposal;
- clear every previous grant;
- retain only that selected proposal's canonical grant;
- if no selectable pending proposal exists, clear the bar and the governed
  diff target immediately;
- if the selected proposal changes, clear a diff belonging to the prior one.

Add a regression that lists proposal P with grant G, then lists P without a
grant/terminal, and proves G cannot enable or submit a later bar.

### 2. Expiry does not disable every action

At the displayed deadline Approve and Reject disable, but Review diff remains
enabled and a previously opened source-bearing diff can remain visible. At the
deadline disable all three actions, invalidate/close the governed diff target,
and immediately re-list. Native remains the only terminal-state authority.
Add component and workbench regressions.

### 3. Error/log boundary is not fully opaque

`normalizeWriteApprovalError` accepts arbitrary host message text when it lacks
path separators, and Workbench logs caught objects. A secret-bearing message
without a slash could reach console output. Map every accepted native error
code to fixed renderer copy, fall back to one fixed unavailable message, and
never log the caught value from list/decision/diff flows. Add an adversarial
console regression whose rejected object contains the grant/token text.

### 4. DTO parsing is looser than the frozen native contract

Fail closed at the Tauri boundary:

- pending proposals require exactly one canonical 43-character base64url
  grant; every non-pending state must omit it;
- `errorCode` must be one of the fixed C3 public error codes;
- `expiresAt` must parse as a finite timestamp;
- diff `byteCount <= 262144`, `lineCount <= 4000`, and actual UTF-8 unified
  bytes must be `<= 262144`;
- write event paths must satisfy the existing portable relative grammar and
  write-event states/error codes must be exact.

Add parser regressions for malformed grants, grant on terminal state, unknown
error codes, invalid timestamps, oversized UTF-8 diff and non-portable event
paths.

### 5. Whitelist reconciliation

The controller authorizes these two necessary C3-E extensions:

- `apps/desktop/src/lib/tauriConversationGateway.ts` for parsing the two
  accepted runtime event kinds;
- `apps/desktop/src/features/conversations/AgentActivityDisclosure.tsx` for
  the compact terminal outcome row.

The added `apps/desktop/src/styles/workbench.css` import is redundant because
the C3 stylesheet already enters through `main.tsx` and component imports.
Remove only that redundant C3 import so this file returns to its prior C2
content. Do not revert any accepted C1/C2 work.

## Write Whitelist

- `apps/desktop/src/lib/writeApprovalGateway.ts`
- `apps/desktop/src/lib/tauriWriteApprovalGateway.ts`
- `apps/desktop/src/lib/tauriConversationGateway.ts`
- `apps/desktop/src/features/conversations/WriteApprovalBar.tsx`
- `apps/desktop/src/features/workbench/Workbench.tsx`
- `apps/desktop/src/features/workbench/ResourcePane.tsx`
- `apps/desktop/src/styles/workbench.css` (remove redundant import only)
- `apps/desktop/tests/write-approval-gateway.test.ts`
- `apps/desktop/tests/write-approval-bar.test.tsx`
- `apps/desktop/tests/write-approval-workbench.test.tsx`
- existing Tauri conversation gateway test file only if required for the new
  strict event parser regression
- `docs/execution/2026-08-01-wave-c3e-approval-ui-report.md`

## Gates And Completion

Run focused C3-E Vitest, any touched gateway regression, full desktop Vitest,
typecheck, production UI build, `git diff --check`, cached-diff check and task
process cleanup. Update the report with each finding and exact evidence. End
with `COMPLETION: DONE` or `COMPLETION: BLOCKED`; worker completion is not
controller acceptance.
