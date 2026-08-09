# Wave C3 — Codex2 MSVC VCTIP process-boundary development brief

## Ownership

- Worker: Codex2 (`gpt-5.6-terra`, high, `fastMode=false`)
- Controller: Codex Desktop
- Stage/commit: forbidden
- Frozen tree: `legacy Keeper (out of tree)` is read-only

## Problem

The controller's real C3 desktop acceptance completed all eight governed-write
scenarios and wrote a passing acceptance record with zero Pillow/port residuals.
The outer `runSupervisedCommand` wrapper then failed because the MSVC compiler
had spawned a detached `vctip.exe` telemetry helper. The wrapper correctly never
kills observation-only descendants, but currently treats every still-live build
descendant as a Pillow cleanup failure.

Observed identity:

`C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\HostX64\x64\VCTIP.EXE`

## Required outcome

Implement the narrowest fail-closed boundary that permits only the exact MSVC
VCTIP telemetry identity after the launcher-owned root has exited. Do not add a
generic ignore callback or a broad process-name allowlist.

The default supervisor behavior must remain unchanged. The opt-in C3 wrapper
must still fail for Pillow, fake ACP, WebView2, port listeners, wrong-path
`vctip.exe`, incomplete identities, PID reuse, and live/mismatched roots. It must
not terminate VCTIP or any numeric observation-only PID.

## Write allowlist

- `scripts/lib/desktop-process.mjs`
- `scripts/lib/desktop-process-safety.test.mjs`
- `apps/desktop/tests/e2e/c3-governed-write.spec.ts`
- `docs/execution/2026-08-02-wave-c3-codex2-vctip-process-boundary-development-report.md`

No other file may be changed.

## Contract

1. Add a named, default-off option whose semantics are specifically limited to
   an MSVC VCTIP telemetry helper.
2. Require both normalized name `vctip.exe` and a normalized executable path
   under a Visual Studio MSVC `VC\Tools\MSVC\...\bin\HostX64\x64\vctip.exe`
   installation shape. A lookalike elsewhere must remain live and fail cleanup.
3. Apply the allowance only to non-root identities and only after the root is
   confirmed dead/replaced. Root safety and identity checks remain first.
4. The C3 E2E wrapper opts in and restores outer desktop cleanliness checking;
   the nested C3 orchestrator remains the owner of its managed child lifecycle.
5. Add RED/GREEN tests for default rejection, exact opt-in acceptance, wrong
   path rejection, root rejection, and preservation of Pillow/fake/WebView
   residual failure behavior.
6. Do not alter C3 product scenarios, timeouts, terminal-record assertions, or
   the internal acceptance orchestrator.

## Verification

Run at minimum:

```powershell
node --test scripts/lib/desktop-process-safety.test.mjs
node --check scripts/lib/desktop-process.mjs
git diff --check -- scripts/lib/desktop-process.mjs scripts/lib/desktop-process-safety.test.mjs apps/desktop/tests/e2e/c3-governed-write.spec.ts
git diff --cached --name-only
```

Do not run the real desktop E2E; the controller owns that acceptance.

## Completion signal

Return `DONE` or `DONE_WITH_CONCERNS` with changed files, RED/GREEN evidence,
tests, residual risk, and confirmation that staging remains empty. Post the same
completion to Keeper and send the controller a completion signal; do not create
a periodic monitor.
