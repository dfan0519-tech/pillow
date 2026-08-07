# C4 D3B3 Write Regression Contract Repair Report

## Root cause

The legacy C3 regression test treated a pending write proposal whose fresh,
authoritative list result omitted its opaque approval grant as a visible,
disabled approval bar. Under the accepted D3B2 single-slot arbitration
contract, that proposal has no decision capability and must be removed from
the candidate set. Production behavior already follows that contract; only the
stale test expectation required repair.

## Exact changed lines

- `apps/desktop/tests/write-approval-workbench.test.tsx:789` removes the unused
  user-event setup from the repaired test.
- `apps/desktop/tests/write-approval-workbench.test.tsx:843-846` replaces the
  disabled-approve expectation after the grant-less authoritative refresh with
  atomic absence checks for the bar and approve action, zero decisions, and no
  serialized-DOM grant.
- `apps/desktop/tests/write-approval-workbench.test.tsx:866-868` retains those
  absence, zero-decision, and no-secret checks after the terminal refresh.

No production code or C4 arbitration test was changed.

## Verification

- Passed: `pnpm --filter @pillow/desktop exec vitest run tests/write-approval-workbench.test.tsx --reporter=verbose` (1 file, 16 tests).
- Passed: `pnpm --filter @pillow/desktop exec vitest run tests/approval-arbitration.test.ts tests/c4-workbench-process-integration.test.tsx tests/write-approval-workbench.test.tsx` (3 files, 45 tests).
- Passed: `pnpm --filter @pillow/desktop typecheck`.
- Passed: `git diff --check -- apps/desktop/tests/write-approval-workbench.test.tsx docs/execution/2026-08-05-wave-c4-d3b3-codex2-write-regression-contract-repair-report.md`.

## Residual risk

This repair deliberately verifies only the write-only stale-grant transition.
The existing C4 integration test continues to prove that a grant-less write
cannot mask an actionable process approval; it passed in the required aggregate
run. Controller acceptance is not asserted by this report.

completion: complete
