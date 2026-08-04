# Wave C3 broad-gate expiry fixture fix brief

Owner: one renderer-test implementation worker

Controller: Codex Desktop, session `<keeper-session>`

Date: 2026-08-01

Status: authorized conditional test-only repair after controller-classified
`HARNESS_ASSERTION_OR_DRIVER` failure

## Objective

Remove the wall-clock expiry time bomb from the C3 write-approval workbench
tests without changing product behavior, approval policy or expiry coverage.

## Controller RED evidence

After the controller accepted focused C3-F real desktop E2E, `pnpm check`
failed in Vitest. A direct focused rerun reproduced 5/16 failures:

```text
opens governed-write diff in the resource pane with truncation metadata
  write-diff-preview missing
submits exact decide payload then clears grant and re-lists
  decideCalls expected 1, received 0
Escape on the bar does not invoke global cancel
  reject decision missing
never logs caught list/decision objects that carry grant or token text
  console.error was not called
normalizes WriteApprovalGatewayError instances so secret messages never reach DOM or console
  alert missing
```

The rendered DOM showed Approve, Reject and Review Diff all disabled.

## Root cause established by controller

`apps/desktop/tests/write-approval-workbench.test.tsx` uses a default pending
fixture with this fixed deadline:

```ts
expiresAt: "2026-08-01T12:10:00.000Z",
```

The controller reproduction occurred at `2026-08-01T14:04Z`. Production
`WriteApprovalBar` correctly computes `expiresAt - Date.now()`, immediately
sets `deadlinePassed`, invokes the expiry callback and disables all three
actions. Every failed expectation depends on clicking one of those disabled
actions.

The same test file already contains the correct expiry-specific pattern:

```ts
const expiresAt = new Date(Date.now() + 5_000).toISOString();
```

Therefore this is a time-dependent test-fixture defect, not a product defect.

## Write allowlist

- `apps/desktop/tests/write-approval-workbench.test.tsx`
- `docs/execution/2026-08-01-wave-c3-broad-gate-expiry-fixture-fix-report.md`

Everything else is read-only. Do not edit:

- `apps/desktop/src/**`;
- any other test file;
- `scripts/**`;
- `crates/**`;
- package manifests, roadmap, checkpoint or prior reports.

Do not stage or commit.

## Required implementation contract

1. Add one small test-only helper near the fixture constants:

   ```ts
   function futureExpiry(minutes = 10): string {
     return new Date(Date.now() + minutes * 60_000).toISOString();
   }
   ```

2. Replace the default `pending()` fixed deadline with `futureExpiry()`.
3. In the oldest-pending ordering fixture, replace the three fixed 2026
   deadlines with distinct future values preserving the intended order:
   `futureExpiry(30)`, `futureExpiry(10)` and `futureExpiry(5)` respectively.
4. Do not change the dedicated expiry test. It must continue using fake timers
   and `Date.now() + 5_000`, then prove actions disable, diff closes and native
   proposals are re-listed.
5. Do not add sleeps, increase `waitFor` timeouts, weaken assertions, remove
   disabled-state checks or change product code.

## Verification

Run:

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/write-approval-workbench.test.tsx
pnpm --filter @pillow/desktop exec vitest run tests/write-approval-workbench.test.tsx tests/project-workbench.test.tsx
pnpm --filter @pillow/desktop test
pnpm --filter @pillow/desktop typecheck
git diff --check -- apps/desktop/tests/write-approval-workbench.test.tsx docs/execution/2026-08-01-wave-c3-broad-gate-expiry-fixture-fix-report.md
git diff --cached --name-only
```

Expected focused results: 16/16, then 32/32. The dedicated expiry test must be
among the passing 16. The controller will independently inspect the diff and
rerun `pnpm check` before acceptance.

## Completion contract

Write:

`docs/execution/2026-08-01-wave-c3-broad-gate-expiry-fixture-fix-report.md`

Include exact changed files, RED/GREEN results, dedicated expiry-test proof,
full desktop Vitest and typecheck results, staging/HEAD, residual processes,
remaining risks and one terminal status: `DONE`, `DONE_WITH_CONCERNS`,
`BLOCKED` or `NEEDS_CONTEXT`.

Send one completion/blocker signal to the controller and Keeper. Do not create
a periodic monitor.
