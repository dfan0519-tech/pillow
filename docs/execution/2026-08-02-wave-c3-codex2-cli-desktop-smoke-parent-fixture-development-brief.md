# Wave C3 — account-2 Codex CLI desktop-smoke parent fixture brief

## Runtime and ownership

Run through independent account-2 Codex CLI (`codex2`, `gpt-5.6-terra`, high,
`fastMode=false`). Controller owns final review and all broad gates.

## Reproduced failure

The real project create/select/restart case passes 1/1 and the repaired Cargo
wrapper case passes 1/1. The remaining `desktop smoke harness` case times out.
Direct `node scripts/smoke-desktop.mjs --self-test` reports:

```text
Desktop cleanup refused: root PID ... was never observed.
```

`nestedProcessScript({ exitParent: true })` currently schedules its parent exit
after 75 ms. On Windows this can occur before the first CIM snapshot completes,
so fail-closed cleanup correctly refuses the never-observed root.

## Required outcome

Give only the parent-exit self-test fixture a bounded, host-appropriate identity
capture window, then still exit normally and prove the detached child is cleaned
through the existing safe lifecycle. Do not weaken `ProcessSupervisor`, change
production smoke behavior, increase the 60-second E2E timeout, or use taskkill /
numeric forced termination.

## Write allowlist

- `scripts/lib/desktop-smoke-self-test.mjs`
- `docs/execution/2026-08-02-wave-c3-codex2-cli-desktop-smoke-parent-fixture-development-report.md`

No other writes. No stage/commit/reset/restore/clean.

## Verification

```powershell
node scripts/smoke-desktop.mjs --self-test
node --test --test-concurrency=1 --test-name-pattern="desktop smoke harness" apps/desktop/tests/e2e/project-launcher.spec.ts
node --check scripts/lib/desktop-smoke-self-test.mjs
node --test scripts/lib/desktop-process-safety.test.mjs
git diff --check -- scripts/lib/desktop-smoke-self-test.mjs
git diff --cached --name-only
```

Run serially, report RED/GREEN with elapsed time, changed files, residual risk,
staging state, owned residual processes and a completion signal. Do not run the
full desktop suite; the controller does that.
