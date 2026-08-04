# Wave C3 — Codex2 incomplete-descendant age development brief

## Ownership and boundary

- Worker: Codex2 (`gpt-5.6-terra`, high, `fastMode=false`)
- Controller: Codex Desktop
- No stage/commit; no real desktop E2E
- Frozen tree: `<workspace>\VaultSystem\keeper`

## Reproduced failure

The controller's real C3 run completed all eight scenarios and wrote a passing
record, but outer cleanup failed on `identity-incomplete` PID 1360. Live CIM
evidence identifies PID 1360 as protected `csrss.exe`, created at system start,
hours before the C3 run. Its numeric `ParentProcessId` collided with a transient
tracked build PID. `ExecutablePath` is unreadable, so `processIdentity` returned
null and `discoverDescendants` recorded it without comparing its otherwise valid
creation tick to the verified parent's tick.

## Required repair

Use the incomplete entry's independently normalized creation tick when it is
available. An incomplete candidate whose creation tick is not strictly later
than the verified parent cannot be that parent's child and must not be tracked.
If the candidate creation tick is missing/invalid, or is strictly later, preserve
the existing fail-closed incomplete-identity behavior. Do not special-case or
allowlist `csrss.exe` or any system process name.

## Write allowlist

- `scripts/lib/desktop-process.mjs`
- `scripts/lib/desktop-process-safety.test.mjs`
- `docs/execution/2026-08-02-wave-c3-codex2-incomplete-descendant-age-development-report.md`

## Verification

Add RED/GREEN tests for:

1. an older incomplete entry with a colliding parent PID is ignored;
2. an equal-tick incomplete entry is ignored;
3. a newer incomplete entry remains tracked and blocks cleanup;
4. a missing/invalid-tick incomplete entry remains tracked and blocks cleanup;
5. all VCTIP, root identity, PID reuse, Pillow/fake/WebView and default safety
   cases remain green.

Run:

```powershell
node --test scripts/lib/desktop-process-safety.test.mjs
node --check scripts/lib/desktop-process.mjs
git diff --check -- scripts/lib/desktop-process.mjs scripts/lib/desktop-process-safety.test.mjs
git diff --cached --name-only
```

Return one completion signal with changed files, RED/GREEN evidence, residual
risk, staging state and Keeper completion. The controller owns source review
and the next real desktop run.
