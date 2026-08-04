# Wave C3 Grok4 expiry-fixture development report

Date: 2026-08-01

Worker: `grok4`
Controller: Codex Desktop (owns final review and broad gates)
Status: **DONE**

## Objective

Own and finalize the test-only repair for wall-clock-dependent governed-write
approval fixtures. Prior broad `pnpm check` reproduced 5/16 failures because a
fixed `expiresAt` timestamp had already elapsed.

## Exact changed files

| Path | Action |
| --- | --- |
| `apps/desktop/tests/write-approval-workbench.test.tsx` | **Kept as candidate** — inspected; no further edit |
| `docs/execution/2026-08-01-wave-c3-grok4-expiry-fixture-development-report.md` | **Created** (this report) |

No product source, scripts, crates, package manifests, or other tests were
modified. Staging was not used. No commit was made.

## Root cause

`pending()` fixtures previously used a fixed ISO deadline:

```ts
expiresAt: "2026-08-01T12:10:00.000Z",
```

Production `WriteApprovalBar` correctly computes `expiresAt - Date.now()`. Once
wall clock passed that instant, the bar set `deadlinePassed`, disabled Approve /
Reject / Review Diff, and every click-dependent assertion failed. This is a
test-fixture time bomb, not a product defect.

## Candidate repair inspection

The working tree already contained the authorized minimum repair:

1. Test-only helper:

   ```ts
   function futureExpiry(minutes = 10): string {
     return new Date(Date.now() + minutes * 60_000).toISOString();
   }
   ```

2. Default `pending()` uses `expiresAt: futureExpiry()` (relative +10 minutes).
3. Oldest-pending ordering fixture preserves relative order via
   `futureExpiry(30)`, `futureExpiry(10)`, `futureExpiry(5)` so
   `selectOldestPendingProposal` (earliest `expiresAt`, then `proposalId`) still
   selects `src/old.ts` for conversation A.
4. Dedicated expiry test remains fake-timer based with
   `new Date(Date.now() + 5_000).toISOString()` and still asserts disabled
   actions, closed governed diff, and re-list after advance.

### Keep vs refine

**Candidate repair was kept without refinement.** No concrete defect was found:

- Relative future offsets remove wall-clock fragility for non-expiry tests.
- Product expiry path is untouched (`WriteApprovalBar` / Workbench selection
  logic unchanged by this worker).
- Dedicated expiry coverage still proves real expiry UX under fake timers.
- No sleeps, weakened assertions, or product changes.

Note: event payload `expires_at: "2026-08-01T12:11:00.000Z"` on
`write_proposal_requested` remains historical timeline metadata only; the bar
deadline is driven by list fixtures, not that payload field.

## Verification (required checks)

| # | Command | Exit | Result |
| --- | --- | --- | --- |
| 1 | `pnpm --filter @pillow/desktop exec vitest run tests/write-approval-workbench.test.tsx` | 0 | **16/16** passed (1 file) |
| 2 | `pnpm --filter @pillow/desktop exec vitest run tests/write-approval-workbench.test.tsx tests/project-workbench.test.tsx` | 0 | **32/32** passed (2 files: 16 + 16) |
| 3 | `pnpm --filter @pillow/desktop exec vitest run tests/write-approval-workbench.test.tsx -t "at expiry disables actions, closes governed diff, and re-lists"` | 0 | **1/1** passed, **15** skipped by name filter |
| 4 | `pnpm --filter @pillow/desktop typecheck` | 0 | `tsc --noEmit` clean |
| 5 | `git diff --check -- apps/desktop/tests/write-approval-workbench.test.tsx docs/execution/2026-08-01-wave-c3-grok4-expiry-fixture-development-report.md` | 0 | no whitespace errors |

Real desktop E2E and repository-wide `pnpm check` were **not** run (controller
owns those after integration).

## Repository and process state

- HEAD: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
- Staging: empty (`git diff --cached --name-only` returned no paths)
- Target test file remains **untracked** in an already-dirty shared worktree;
  this worker did not stage, commit, reset, restore, or clean
- After verification: no owned `vitest` / `write-approval-workbench` Node
  process remains

## Remaining risks

1. Shared dirty worktree: many unrelated modified/untracked files exist;
   controller must not treat this worker as provenance for those paths.
2. The focused test file was already untracked before this session; independent
   historical diff isolation is limited — controller review remains the
   acceptance authority for integration.
3. Relative `futureExpiry(10)` can still theoretically expire if a single test
   process stalled for more than ten minutes mid-case; practical risk is low
   (suite completes in seconds). A longer default is unnecessary for current
   focused runs.
4. Broad Vitest / full desktop E2E remain controller gates; this worker only
   proved the focused allowlisted suite.

## Completion signal

```text
status: DONE
changed_files:
  - apps/desktop/tests/write-approval-workbench.test.tsx  (kept candidate; no edit)
  - docs/execution/2026-08-01-wave-c3-grok4-expiry-fixture-development-report.md  (created)
root_cause: fixed pending expiresAt wall-clock time bomb in test fixtures
repair: KEPT candidate futureExpiry(minutes) without refinement
checks:
  write-approval-workbench: 16/16 exit 0
  write-approval + project-workbench: 32/32 exit 0
  dedicated expiry: 1/1 (15 skipped) exit 0
  typecheck: exit 0
  git diff --check (allowlist): exit 0
staging: empty
owned_test_processes: none
```
