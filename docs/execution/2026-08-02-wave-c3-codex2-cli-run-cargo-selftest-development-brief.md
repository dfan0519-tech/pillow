# Wave C3 — account-2 Codex CLI run-cargo self-test development brief

## Runtime identity

This task must run through the independent account-2 Codex CLI profile
(`codex2`, `gpt-5.6-terra`, high, `fastMode=false`). It is not a Codex Desktop
conversation subagent.

Before work, Keeper boot with `project=pillow`, task
`wave-c3-run-cargo-selftest`, `agentName=codex2-cli`, `agentRole=worker`.

## Reproduced failure

`project-launcher.spec.ts` times out its cargo-wrapper case at 60 seconds.
Direct `node scripts/run-cargo.mjs --self-test` eventually reports:

```text
Desktop cleanup refused: root PID ... was never observed.
```

The first self-test fixture launches `node -e process.exit(7)`. It can exit
before the Windows CIM snapshot in `ProcessSupervisor` captures an immutable
root identity. Fail-closed cleanup then correctly rejects the never-observed
root. Production `pnpm check` Cargo test/clippy paths are green.

## Required outcome

Make the exit-code fixture remain alive long enough for a normal Windows CIM
identity capture, then exit with code 7 so the same preservation assertion is
still meaningful. Use a bounded deterministic fixture delay appropriate for
this host. Do not weaken `ProcessSupervisor`, ignore a never-observed root,
change production Cargo execution, or increase the E2E wrapper's 60-second
timeout.

## Write allowlist

- `scripts/run-cargo.mjs`
- `docs/execution/2026-08-02-wave-c3-codex2-cli-run-cargo-selftest-development-report.md`

No other file may be changed. No stage/commit/reset/restore/clean.

## Required verification

```powershell
node scripts/run-cargo.mjs --self-test
node --test --test-concurrency=1 --test-name-pattern="cargo wrapper" apps/desktop/tests/e2e/project-launcher.spec.ts
node --check scripts/run-cargo.mjs
node --test scripts/lib/desktop-process-safety.test.mjs
git diff --check -- scripts/run-cargo.mjs
git diff --cached --name-only
```

Run real process tests serially. Do not run the full desktop suite. Report
RED/GREEN evidence, elapsed time, changed files, residual risk, staging state,
owned residual processes and Keeper completion. Emit one completion signal.
