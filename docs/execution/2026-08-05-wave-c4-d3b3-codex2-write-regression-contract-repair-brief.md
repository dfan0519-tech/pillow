# C4 D3B3 Aggregate Gate Repair — Codex2 Brief

## Controller finding

`pnpm check` and the isolated file both fail the legacy C3 test
`atomically drops a raw grant when the next list omits it or terminalizes`.
The test expects a grant-less pending write to remain visible as a disabled
write approval bar.

That expectation conflicts with the accepted D3B2 single-slot arbitration
contract and current production implementation: a grant-less write has
`decisionAvailable: false`, must not win arbitration, and must disappear so an
actionable process approval can occupy the single slot.

Production behavior is correct. Repair only the stale regression test.

## Exact write allowlist

- `apps/desktop/tests/write-approval-workbench.test.tsx`
- `docs/execution/2026-08-05-wave-c4-d3b3-codex2-write-regression-contract-repair-report.md` (create)

Do not modify production code or any other file. Preserve the dirty tree. Do
not stage, commit, reset, restore, clean, or broadly format.

## Required behavior

1. Keep the initial proof that a pending write carrying a raw grant exposes an enabled approve action.
2. After the authoritative same-scope list returns the same pending proposal without a grant, assert atomically that the write approval bar and approve action are absent, no decision was sent, and the secret grant is absent from serialized DOM.
3. After the authoritative list terminalizes the proposal, retain the absent-bar/no-decision/no-secret proof.
4. Do not weaken the accepted C4 tests proving that a grant-less write cannot mask an actionable process approval.

## Required verification

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/write-approval-workbench.test.tsx --reporter=verbose
pnpm --filter @pillow/desktop exec vitest run tests/approval-arbitration.test.ts tests/c4-workbench-process-integration.test.tsx tests/write-approval-workbench.test.tsx
pnpm --filter @pillow/desktop typecheck
git diff --check -- apps/desktop/tests/write-approval-workbench.test.tsx docs/execution/2026-08-05-wave-c4-d3b3-codex2-write-regression-contract-repair-report.md
```

Write the report with root cause, exact changed lines, tests, residual risk and
`completion: complete|blocked`. Do not claim controller acceptance.
