# C4 D3B3 — Grok4 run-cargo cleanup race report

## Status

Complete. Worker-local gates green. Controller owns desktop E2E acceptance.

## Root cause

`ProcessSupervisor.cleanup()` on Windows could false-fail a successful
`run-cargo` natural exit with:

```text
Desktop cleanup refused: observation-only PIDs remain [<pid>]
```

Two cooperating races produced that refusal after product tests had already
passed:

1. **Post-last-poll death window.** The cleanup loop always delayed after the
   final in-budget snapshot, then refused from that snapshot’s state without a
   fresh observation. A fully identified observation-only descendant that died
   during that last delay (common for short-lived Cargo/`rustc` helpers) was
   still recorded as `observed`, so cleanup threw even though the next CIM
   snapshot would have proven the exact identity absent.

2. **Natural root exit + kill-on-handle.** When the launcher-owned
   `ChildProcess` had already exited, cleanup still treated a live root
   identity as `liveRoot` and called `child.kill()`. On Windows that returns
   `false` for an exited handle, which could refuse a successful natural exit
   path. Separately, a root that exited before the first complete CIM capture
   remained `never-observed` and failed closed even though no kill was
   required and the stable handle already reported exit.

Neither case needed numeric-PID or name-wide termination. Success only needs
a fresh snapshot plus the recorded immutable identity (or the stable root
handle when it already exited).

## Why the repair is safe

- Termination remains launcher-owned: only the root `ChildProcess` handle may
  receive `kill()`, and only while that handle has not already exited.
- No `taskkill`, numeric-PID kill, name-wide kill, WMI deletion, or fixed sleep
  as a success mechanism.
- Success still requires a fresh snapshot decision:
  - complete identities must be `confirmed-dead` or `identity-replaced`
    (or an opted-in detached MSVC VCTIP residual);
  - incomplete / never-observed descendants still fail closed;
  - root identity mismatch, system-process roots, and unbound Pillow/port
    residuals still fail closed;
  - still-live observed descendants still fail with
    `observation-only PIDs remain`.
- A never-observed root may succeed only when the launcher handle has already
  exited (no kill issued, no descendants gained ownership). A still-live
  never-observed root remains fail-closed.
- After the in-budget polls, one final capture reassesses state so a descendant
  that dies during the last delay is proven absent before refusal.

## Changed files (allowlist only)

| File | Change |
| --- | --- |
| `scripts/lib/desktop-process.mjs` | Extract `assessWindowsCleanup` / `requestRootTermination` / `refuseWindowsCleanupExhausted`; final post-delay snapshot; natural-exit handle authority; never-observed + exited root success |
| `scripts/lib/desktop-process-safety.test.mjs` | Regression fixtures for natural exit, still-live descendant, final-delay absence, kill-after-exit, never-observed exited root; residual observation count for final poll |
| `scripts/run-cargo.mjs` | Self-test natural successful child + short-lived descendant path |
| `docs/execution/2026-08-05-wave-c4-d3b3-grok4-run-cargo-cleanup-race-report.md` | This report |

## Tests and exit codes

| Command | Exit |
| --- | --- |
| `node scripts/lib/desktop-process-safety.test.mjs` | `0` (46/46) |
| `node scripts/run-cargo.mjs --self-test` | `0` (`naturalChildCleanup`, `signalCleanup`, MSVC passed) |
| `pnpm check` | `0` (workspace, typecheck, 323 Vitest, cargo test --workspace, clippy -D warnings) |
| `git diff --check --` allowlisted paths | `0` |

First `pnpm check` attempt failed only because this grok4 slot had no active
rustup toolchain under the isolated `USERPROFILE` (`rustup could not choose a
version of cargo`), which also surfaced the never-observed fast-exit path.
Retry used the host toolchains at `<host-user-profile>\.rustup` /
`<host-user-profile>\.cargo` for the gate only; no repo config was changed.

## Remaining risk

- A descendant that remains present with a matching complete identity for the
  full observation budget plus the final snapshot still correctly fails closed.
  Long-lived detached helpers outside the VCTIP opt-in remain residuals by
  design.
- CIM snapshot latency still bounds wall-clock cleanup time; correctness is
  snapshot-based, not a fixed sleep.
- Desktop E2E was not run in this task (controller gate).

## Tree hygiene confirmation

- No path outside the write allowlist was modified.
- No files were staged, committed, reset, cleaned, or restored.
- Pre-existing dirty-tree work outside the allowlist was left untouched.
- Staging area remained empty (`git diff --cached --name-only` empty).
