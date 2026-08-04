# Wave C3 Grok4 expiry-fixture development brief

Date: 2026-08-01

## Ownership

- Worker: `grok4` only.
- Controller: Codex Desktop; controller owns final review and all broad gates.
- Repository: `<workspace>\pillow`.
- Frozen reference: `<workspace>\VaultSystem\keeper`; read-only, never modify it.

## Objective

Own and finalize the test-only repair for the wall-clock-dependent governed-write
approval fixtures. The prior broad run reproduced 5/16 failures because a fixed
`expiresAt` timestamp had elapsed. The current working tree contains a candidate
`futureExpiry(minutes)` repair. Inspect it as development work, keep it if it is
the minimum correct fix, or refine it within the allowlist if a concrete defect
is found.

The product expiry behavior must remain unchanged. The dedicated fake-timer
expiry test must still prove that an actually expired proposal disables actions,
closes the governed diff, and re-lists proposals.

## Writable allowlist

- `apps/desktop/tests/write-approval-workbench.test.tsx`
- `docs/execution/2026-08-01-wave-c3-grok4-expiry-fixture-development-report.md`

Everything else is read-only. Do not stage, commit, reset, restore, clean, kill
processes by name, or rewrite unrelated dirty files.

## Required checks

1. `pnpm --filter @pillow/desktop exec vitest run tests/write-approval-workbench.test.tsx`
2. `pnpm --filter @pillow/desktop exec vitest run tests/write-approval-workbench.test.tsx tests/project-workbench.test.tsx`
3. `pnpm --filter @pillow/desktop exec vitest run tests/write-approval-workbench.test.tsx -t "at expiry disables actions, closes governed diff, and re-lists"`
4. `pnpm --filter @pillow/desktop typecheck`
5. `git diff --check -- apps/desktop/tests/write-approval-workbench.test.tsx docs/execution/2026-08-01-wave-c3-grok4-expiry-fixture-development-report.md`

Do not run real desktop E2E or repository-wide `pnpm check`; the controller will
run those after all development is integrated.

## Completion contract

Write the report in the allowlisted path and return one completion signal with:

- status: `DONE`, `DONE_WITH_CONCERNS`, or `BLOCKED`;
- exact changed files;
- root cause and whether the candidate repair was kept or refined;
- exact test counts and exit status;
- remaining risks;
- confirmation that staging is empty and no owned test process remains.
