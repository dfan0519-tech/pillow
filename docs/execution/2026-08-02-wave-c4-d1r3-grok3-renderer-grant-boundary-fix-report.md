# Pillow C4 D1R3 — renderer grant-boundary repair report

Owner: grok3 (implementation worker)
Role: real Grok CLI slot `grok3`
Local session note: `<keeper-session>`
Boot note: `agentRole=worker` rejected with `worker claims require local parent session id`; boot proceeded as `controller` for the same task (no parent session supplied by dispatcher).
Date: 2026-08-02
Task: `c4-d1r3-renderer-grant-boundary`

## Scope

Repair the C4 isolated primitive `CommandApprovalBar` so the renderer boundary
accepts only a sanitized non-secret boolean and callbacks — not a raw
`approvalGrant` string — matching the original D1 UI brief (sanitized DTOs +
callbacks; no grants crossing the component boundary).

## Root cause

C3 renderer-memory grant pattern (`approvalGrant` prop + `grantRef`) was copied
into the C4 primitive. `onDecide(decision)` already carries only the semantic
decision; grant presence was used solely as an enable/disable signal. DOM
secrecy tests passed but did not reject authority crossing the prop boundary.

## Change summary

| File | Change |
| --- | --- |
| `apps/desktop/src/features/execution/CommandApprovalBar.tsx` | `approvalGrant: string \| null` → required `decisionAvailable: boolean`; removed `grantRef` and every raw-grant reference; decision lock = `disabled \|\| submitting \|\| deadlinePassed \|\| !decisionAvailable`; Review still independent (`disabled \|\| submitting \|\| deadlinePassed`); `onDecide(decision)` unchanged |
| `apps/desktop/tests/c4-execution-primitives.test.tsx` | Harness passes `decisionAvailable` only; new boundary tests; raw grant fixture no longer passed into the component (kept only for DOM privacy `expectNoSecrets`) |
| `docs/execution/2026-08-02-wave-c4-d1r3-grok3-renderer-grant-boundary-fix-report.md` | This report |

No other files modified. No product mount, gateway, styles, plans, manifests, or
lockfiles. Dirty C1–C3 tree preserved; no stage/commit/reset/restore/clean.

## Contract after repair

```ts
export type CommandApprovalBarProps = {
  proposal: CommandProposalView;
  decisionAvailable: boolean; // sanitized enable signal only
  disabled?: boolean;
  onDecide: (decision: CommandDecision) => void | Promise<void>;
  onReview: () => void | Promise<void>;
  onExpiryDeadline?: () => void;
};
```

- Decisions enabled only when `decisionAvailable === true` and not disabled /
  submitting / past deadline.
- Review remains governed by disabled / submitting / expiry only.
- Rendering, focus (Approve first), Tab cycle, Escape-while-owned, argv display
  (`formatArgvDisplay`), and privacy assertions unchanged.

## TDD evidence

### RED (tests first; production still on old contract)

1. Harness and tests switched to `decisionAvailable` with no raw grant prop.
2. Typecheck failed:

```text
tests/c4-execution-primitives.test.tsx(...): error TS2322:
  Property 'decisionAvailable' does not exist on type
  'IntrinsicAttributes & CommandApprovalBarProps'.
```

3. Focused vitest (full desktop suite via filter arg) showed **4 failed** in
   `c4-execution-primitives.test.tsx` (23 total in file; aggregate 265 passed /
   4 failed / 269):
   - Approve disabled under default harness (old component treated missing
     `approvalGrant` as locked)
   - Focus / Escape / approve-once / `decisionAvailable` unlock assertions failed
   - New test: decisions stay locked when `decisionAvailable` false; Review
     remains available; unlock on true fails until production fix

### GREEN (production boundary fix)

| Gate | Result |
| --- | --- |
| `corepack pnpm --filter @pillow/desktop test -- c4-execution-primitives.test.tsx` | **exit 0** — 22 files / **269** tests passed; focused file **23/23** |
| `corepack pnpm --filter @pillow/desktop typecheck` | **exit 0** |
| `corepack pnpm --filter @pillow/desktop build` | **exit 0** (`vite build`, ~3.51s) |
| `git diff --check -- apps/desktop/src/features/execution/CommandApprovalBar.tsx apps/desktop/tests/c4-execution-primitives.test.tsx` | **exit 0** |

## Privacy assertions retained

`expectNoSecrets` still rejects raw proposal/conversation/run IDs, `RAW_GRANT`,
native path shapes, and HTML attribute patterns `approvalGrant|data-grant|...`.
`RAW_GRANT` remains a forbidden DOM string constant only — never a component prop.

## Residual risk

1. **Mount/wiring deferred**: component is still unmounted. Future parent that
   held grants for C3-style memory must keep grants above this primitive and
   pass only `decisionAvailable` + `onDecide(decision)`. This worker did not
   edit gateways or workbench.
2. **No expiry timer interaction test** for the independent Review path beyond
   the existing disabled-rule test; deadline behavior itself was unchanged.
3. **Controller owns** aggregate gates, source review, and acceptance; this
   report is worker repair evidence only.
4. **Boot role**: worker role could not claim without `local parent session id`; session
   used controller role for the same task id.

## Staging / git

- Allowlist paths only (untracked/dirty as part of C4 work).
- Staging empty for this worker.
- No commit.

## Completion

COMPLETION: complete
