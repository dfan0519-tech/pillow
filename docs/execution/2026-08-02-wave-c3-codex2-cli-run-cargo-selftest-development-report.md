# Wave C3 account-2 Codex CLI run-cargo self-test development report

Status: DONE_WITH_CONCERNS

Date: 2026-08-02

## Scope and repair

This worker changed only the authorized fixture line in
`scripts/run-cargo.mjs` and created this required report. The existing
uncommitted `ProcessSupervisor` constructor-call change in that script was
preserved untouched.

The exit-code fixture now runs
`setTimeout(() => process.exit(7), 2_000)`. The bounded two-second delay is
four times the wrapper's 500 ms capture cadence, giving Windows CIM a normal
identity-capture window while retaining the asserted exit code of `7`.
No production Cargo command, `ProcessSupervisor` behavior, fail-closed
never-observed handling, or E2E timeout changed.

## RED/GREEN evidence

RED: before the fixture adjustment,
`node scripts/run-cargo.mjs --self-test` exceeded the 60-second E2E boundary
and the local 124-second command budget. This is the reproduced
never-observed-root cleanup path; the process was stopped by the command
budget before it could emit its eventual refusal.

GREEN:

- `node scripts/run-cargo.mjs --self-test` — exit 0 in 7.43 s; reports
  `exitCode`, `msvcToolchain`, and `signalCleanup` as `passed`.
- `node --test --test-concurrency=1 --test-name-pattern="cargo wrapper" apps/desktop/tests/e2e/project-launcher.spec.ts`
  — exit 0 in 9.71 s; 1 passed, 0 failed.
- `node --check scripts/run-cargo.mjs` — exit 0 in 0.05 s.
- `node --test scripts/lib/desktop-process-safety.test.mjs` — exit 0 in
  0.18 s; 41 passed, 0 failed.
- `git diff --check -- scripts/run-cargo.mjs` — exit 0.
- `git diff --cached --name-only` — no staged paths.

Real process tests were run serially. The full desktop suite was not run.

## Boundary and residual risk

No stage, commit, reset, restore, or clean operation was performed. No owned
`node`, `cargo`, or `rustc` process matching the self-test command or its
descendant marker remained after verification.

The two-second window is deterministic and sufficient on this host, but it is
not an architectural change to CIM scheduling; an exceptionally overloaded
Windows host could still delay a snapshot beyond the fixture window. The
focused E2E confirms the normal host path completes far below its fixed
60-second limit.

## Keeper completion concern

The brief-required exact Keeper boot payload was posted to
local session bridge boot (optional) and was rejected with HTTP 400:
`worker claims require local parent session id`. The brief forbids adding that field,
so no valid worker session was issued. A genuine Keeper completion message
requires that session ID and therefore was not posted; no fabricated or
borrowed session identity was used. A sessionless completion draft was posted
to `/api/completion-draft` successfully (HTTP 200); Keeper reported
`shouldIngest: false` because no canonical project target was available.
