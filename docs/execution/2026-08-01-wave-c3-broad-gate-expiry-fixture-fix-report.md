# Wave C3 broad-gate expiry fixture fix report

Date: 2026-08-01

Status: DONE_WITH_CONCERNS

## Scope

Changed files:

- `apps/desktop/tests/write-approval-workbench.test.tsx`
- `docs/execution/2026-08-01-wave-c3-broad-gate-expiry-fixture-fix-report.md`

The test-only `futureExpiry(minutes = 10)` helper now supplies the default
pending fixture deadline. The oldest-pending fixture uses 30-, 10-, and
5-minute future values in the existing intended order. No product code,
assertion, timeout, or dedicated expiry-test logic changed.

## RED evidence

Before the repair, the focused test command exited 1 with 5 failures of 16.
The rendered approval actions were disabled because the fixed pending-fixture
deadline had already elapsed.

## GREEN evidence

| Command | Result |
| --- | --- |
| `pnpm --filter @pillow/desktop exec vitest run tests/write-approval-workbench.test.tsx` | PASS: 16/16 |
| `pnpm --filter @pillow/desktop exec vitest run tests/write-approval-workbench.test.tsx tests/project-workbench.test.tsx` | PASS: 32/32 |
| dedicated `-t "at expiry disables actions, closes governed diff, and re-lists"` | PASS: 1/1 (15 skipped by name filter) |
| `pnpm --filter @pillow/desktop test` | PASS: 21 files, 246/246 tests |
| `pnpm --filter @pillow/desktop typecheck` | PASS (`tsc --noEmit`) |
| `git diff --check -- apps/desktop/tests/write-approval-workbench.test.tsx docs/execution/2026-08-01-wave-c3-broad-gate-expiry-fixture-fix-report.md` | PASS: exit 0 |

The dedicated expiry test remains fake-timer based and continues to construct
its deadline with `new Date(Date.now() + 5_000).toISOString()`. Its passing
result proves the expiry path still disables actions, closes the governed diff,
and re-lists native proposals.

## Repository and process state

- HEAD: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
- Staging: `git diff --cached --name-only` returned no paths; nothing was
  staged or committed.
- Target files are untracked in an already-dirty shared worktree.
- No `vitest` or `write-approval-workbench` Node process remained after
  verification.

## Remaining risks

The repository contains extensive pre-existing, unrelated dirty changes. This
repair intentionally did not alter or validate their provenance. The focused
test file was also already untracked before this worker's edit, so controller
diff review remains the acceptance authority.
