# Pillow C4 D1R3 — renderer grant-boundary repair brief

## Role and boundary

- Worker: real Grok CLI slot `grok3`.
- Keeper task: `c4-d1r3-renderer-grant-boundary`; boot as `agentRole=worker` before editing.
- Repository: `this repository` only. Keeper and reference repositories are read-only.
- Preserve the accepted dirty C1–C3 working tree. Do not stage, commit, reset, restore or clean.

## Controller rejection evidence

All aggregate tests are green, but controller source review rejects the current
`CommandApprovalBar` boundary:

- The original D1 brief requires controlled components to receive sanitized
  DTOs and callbacks only and not expose grants.
- `CommandApprovalBarProps` currently accepts raw `approvalGrant: string | null`.
- The component mirrors that raw grant in `grantRef` inside the renderer.
- The grant is not needed to perform the callback: `onDecide` already accepts
  only the semantic decision. The renderer merely uses grant presence as an
  enable/disable signal.

Root cause: the C3 renderer-memory grant pattern was copied into an isolated C4
primitive even though the C4 D1 brief narrowed this layer to sanitized DTOs and
callbacks. Existing tests asserted DOM secrecy but did not reject authority
crossing the component boundary.

## Required TDD repair

1. Read all three AGENTS files, the C4 design, the original D1 UI brief/report,
   this brief, and the full current component/test files.
2. RED first: change the focused test harness to pass a non-secret boolean
   `decisionAvailable` and no raw grant. Add/adjust a test proving decisions are
   enabled only when this sanitized boolean is true and Review remains governed
   by the existing independent disabled/expiry rules. Run the focused test and
   typecheck; observe the old component contract fail before production edits.
3. GREEN: in `CommandApprovalBar.tsx`:
   - replace `approvalGrant` with required `decisionAvailable: boolean`;
   - remove `grantRef` and every raw-grant reference;
   - compute decision locking from `disabled`, `submitting`, deadline and
     `decisionAvailable` only;
   - keep `onDecide(decision)` unchanged;
   - do not change rendering, focus, expiry, Escape or argv display behavior.
4. Remove raw grant fixture plumbing from this focused test when no longer
   needed. Do not weaken DOM/native-path/raw-ID privacy assertions.
5. Run:
   - `corepack pnpm --filter @pillow/desktop test -- c4-execution-primitives.test.tsx`
   - `corepack pnpm --filter @pillow/desktop typecheck`
   - `corepack pnpm --filter @pillow/desktop build`
   - `git diff --check -- apps/desktop/src/features/execution/CommandApprovalBar.tsx apps/desktop/tests/c4-execution-primitives.test.tsx`
6. Report RED evidence, GREEN evidence, exact changed files and residual risk.

## Exclusive write allowlist

- `apps/desktop/src/features/execution/CommandApprovalBar.tsx`
- `apps/desktop/tests/c4-execution-primitives.test.tsx`
- `docs/execution/2026-08-02-wave-c4-d1r3-grok3-renderer-grant-boundary-fix-report.md`

No other file may change. Do not mount the component or edit gateways, styles,
plans, manifests or lockfiles.

## Completion signal

Write the report and emit one completion message containing
`COMPLETION: complete` or a precise blocker. The controller owns source review,
aggregate gates and acceptance.
