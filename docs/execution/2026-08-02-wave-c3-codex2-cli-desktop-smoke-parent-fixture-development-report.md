# Wave C3 Codex2 CLI desktop-smoke parent-fixture development report

Status: DONE_WITH_CONCERNS

Date: 2026-08-02

Keeper worker session: `<keeper-session>`

## Scope

The only files written by this worker are the brief allowlist:

- `scripts/lib/desktop-smoke-self-test.mjs`
- `docs/execution/2026-08-02-wave-c3-codex2-cli-desktop-smoke-parent-fixture-development-report.md`

For the parent-exit fixture only, the parent now remains alive for 500 ms,
which gives the Windows CIM identity capture a bounded window. Its detached
child remains alive for a bounded 5 seconds, so the existing safe supervisor
lifecycle observes it and confirms its natural exit after the parent exits.
The normal root/descendant fixture remains an unbounded child and is unchanged.
No production smoke behaviour, `ProcessSupervisor`, E2E timeout, taskkill, or
numeric forced termination was changed or used.

The shared worktree already contained the `rootPid` to launcher-owned `child`
constructor migration in this same source file before this task. That
pre-existing dirty hunk remains visible in the target diff and is not this
worker's fixture change.

## RED / GREEN evidence

| Phase | Command | Result | Elapsed |
| --- | --- | --- | --- |
| RED | `node scripts/smoke-desktop.mjs --self-test` with the original 75 ms parent exit | host command timeout, exit 124; no success marker | 34.03 s |
| fixture refinement | 500 ms parent with an unbounded detached child | fail-closed `observation-only PIDs remain [29640]` | 74.60 s |
| fixture refinement | bounded child at 1,000 ms | fail-closed `never-observed descendant PID 19172` | 16.37 s |
| GREEN | `node scripts/smoke-desktop.mjs --self-test` | exit 0; all six self-test fields passed | 19.95 s |
| GREEN | `node --test --test-concurrency=1 --test-name-pattern="desktop smoke harness" apps/desktop/tests/e2e/project-launcher.spec.ts` | exit 0; 1 passed, 0 failed | 22.57 s |
| GREEN | `node --check scripts/lib/desktop-smoke-self-test.mjs` | exit 0 | 0.05 s |
| GREEN | `node --test scripts/lib/desktop-process-safety.test.mjs` | exit 0; 41 passed, 0 failed | 0.17 s |
| GREEN | `git diff --check -- scripts/lib/desktop-smoke-self-test.mjs` | exit 0 | 0.04 s |
| GREEN | `git diff --cached --name-only` | exit 0; no paths | 0.05 s |

The passing focused outer test confirms the 60-second wrapper remains unchanged
and completes in 22.57 seconds. The direct self-test reports
`cleanupParentExited: "passed"` and all other required self-test fields.

## Repository and process state

- HEAD: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
- Staging: empty. No stage, commit, reset, restore, or clean operation ran.
- Final green runs left no listener on TCP 1420 and no Pillow process.
- The machine has seven generic detached `node -e setInterval(...)` processes
  from earlier shared-worktree runs. Five predate this task. Two are
  attributable to this worker's interrupted RED/refinement runs: PID `22640`
  (02:30:54) and PID `29640` (02:32:03). Their launcher handles were lost when
  the host cancelled the failing commands; they were not forcibly terminated,
  consistent with the safety boundary.
- Existing WebView2 and VCTIP processes were present before the final green
  fixture run and were not modified.

## Residual risk

The 500 ms parent and 5,000 ms detached-child fixture timings are
Windows-host-specific bounded test data, not a production lifecycle change.
The controller should perform its own source review and broad desktop gates.
The two attributable orphaned generic Node processes above require a separate,
explicitly authorized cleanup path because this worker no longer owns a safe
child handle.

## Completion signal

`DONE_WITH_CONCERNS — fixture-only parent identity window is green; focused harness 1/1 and process safety 41/41 pass; staging is empty. Concerns: two generic Node children from interrupted RED/refinement runs remain without safe handles, and the shared source diff contains pre-existing constructor migration hunks.`
