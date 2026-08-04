# Wave C3-E Brief — Conversation Approval And Diff UI

Owner: grok2 implementation worker

Controller session: `<keeper-session>`

Worker session: `<keeper-session>`

Status: ACTIVE. C3-D is controller-accepted and the native DTO below is
frozen. Renderer files only.

## Controller UI Addendum

The types below exactly mirror the accepted C3-D camel-case serialization.
They are authoritative; do not add renderer-only authority fields or
compatibility aliases.

## Goal

Show one conversation-owned approval bar, open the host-generated bounded diff
in the existing contextual resource pane, submit an exact one-shot decision,
and retain a compact terminal outcome in the owning timeline.

## Typed Gateway

```ts
export type WriteDecision = "approve_once" | "reject";
export type WriteMode = "create" | "modify";
export type WriteProposalState =
  | "pending_approval" | "approved" | "applying" | "succeeded"
  | "rejected" | "expired" | "cancelled" | "interrupted" | "failed";

export type WriteProposalView = {
  proposalId: string;
  conversationId: string;
  path: string;
  mode: WriteMode;
  state: WriteProposalState;
  summary: string;
  expiresAt: string;
  approvalGrant?: string;
  errorCode?: string;
};

export type WriteDiffView = {
  proposalId: string;
  path: string;
  mode: WriteMode;
  unified: string;
  truncated: boolean;
  lineCount: number;
  byteCount: number;
};
```

Gateway invokes exactly `list_write_proposals`, `decide_write_proposal`, and
`open_write_proposal_diff`. No root/content/base/digest/fingerprint field may
cross from renderer to native.

The opaque `approvalGrant` may exist in component state only for the exact
owning pending proposal. It must never be rendered into the DOM, copied to a
data attribute, placed in an error, analytics/debug object, console call,
timeline event, local/session storage, or test snapshot. Send it only as the
exact native decision argument, then clear the local reference after the
native call settles and reconcile by listing again.

## Interaction Contract

- Filter by selected conversation; never show foreign proposals.
- At most one pending bar above composer; no historical card stack.
- Show portable path, create/modify, expiry, `Approve once`, `Reject`, and
  `Review diff`.
- Initial focus on Approve. Tab cycles inside actions. Escape invokes Reject
  only while this bar owns focus; it must not leak to global stop.
- Disable actions after submit. Reconcile from durable/native result rather
  than optimistic-only state.
- Switch away parks state; return re-lists and restores the exact live bar.
- Diff opens in existing collapsible resource pane and respects project/
  conversation generation guards and minimum conversation width.
- Terminal timeline row shows concise outcome; success alone offers Open file.
- No allow-session, raw JSON/content, native paths, raw provider output,
  proposal/session/run ids as visible text, or debug metadata.

If more than one live proposal exists for the same selected conversation,
show exactly the oldest by `expiresAt`, then stable `proposalId` tie-break;
resolving it reveals the next. Never stack bars and never discard the parked
native proposals.

The UI does not own expiry. At the displayed deadline it disables actions and
immediately re-lists; only the native result may mark the proposal expired.
There is no renderer timer that submits a decision or invents a terminal
state.

Re-list on selected-conversation entry and on accepted
`write_proposal_requested` / `write_proposal_resolved` runtime events. Do not
add recurring polling. Every async list/diff/decision result must carry the
existing project/conversation generation guard and be discarded when the
selection changes before completion.

`Review diff` opens a dedicated governed-write diff target in the existing
context pane. It must not reinterpret the diff as a normal resource read,
must close or become inert on project/conversation switch, and must show the
host truncation flag/counts. The diff text is source-bearing: do not log it or
include it in snapshots beyond focused fixture assertions.

Terminal rows derive from the durable runtime events/native state, not an
optimistic decision. Only `succeeded` offers `Open file`; it must route the
portable path through the existing selected-project resource gateway. All
other terminal states use fixed path-free explanatory copy.

## File Whitelist

- `apps/desktop/src/lib/writeApprovalGateway.ts`
- `apps/desktop/src/lib/tauriWriteApprovalGateway.ts`
- `apps/desktop/src/features/conversations/WriteApprovalBar.tsx`
- `apps/desktop/src/features/workbench/WriteDiffPreview.tsx`
- `apps/desktop/src/styles/write-approval.css`
- `apps/desktop/tests/write-approval-gateway.test.ts`
- `apps/desktop/tests/write-approval-bar.test.tsx`
- `apps/desktop/tests/write-diff-preview.test.tsx`
- `apps/desktop/tests/write-approval-workbench.test.tsx`
- `apps/desktop/src/lib/conversationGateway.ts`
- `apps/desktop/src/features/conversations/agentActivityTypes.ts`
- `apps/desktop/src/features/conversations/agentActivityFromTimeline.ts`
- `apps/desktop/src/features/conversations/agentActivityProjection.ts`
- `apps/desktop/src/features/conversations/ConversationTimeline.tsx`
- `apps/desktop/src/features/workbench/Composer.tsx`
- `apps/desktop/src/features/workbench/ResourcePane.tsx`
- `apps/desktop/src/features/workbench/Workbench.tsx`
- `apps/desktop/src/App.tsx`
- `apps/desktop/src/main.tsx`
- `docs/execution/2026-08-01-wave-c3e-approval-ui-report.md`

Existing App/Workbench tests may be modified only when compilation or a new
selection/isolation regression requires it; list each such file and why in the
report. Do not broaden into unrelated shell redesign.

Do not touch Rust/native files.

## TDD And Gates

Write RED tests for exact invokes, opaque errors, foreign filtering, one bar,
focus/Tab/Escape, expiry, disabled submit, terminal reconcile, project switch,
diff truncation and absence of raw IDs/paths/output. Then implement minimally.

Include tests for deterministic oldest-proposal selection, no interval
polling, stale async list/diff/decision suppression, grant absence from DOM and
console/snapshots, grant clearing after settle, Escape not reaching global
cancel while the bar owns focus, Escape doing nothing when focus is outside,
and success-only `Open file` routing through the project-scoped resource
gateway.

Run focused Vitest, complete desktop Vitest if feasible, typecheck, production
UI build and diff/staging checks.

## Completion

Write only `docs/execution/2026-08-01-wave-c3e-approval-ui-report.md` with
changed files, RED/GREEN evidence, screenshots if produced, risks and
`COMPLETION: DONE` or `COMPLETION: BLOCKED`.
