# Wave C3-F real-profile snapshot harness fix report

Status: DONE

Date: 2026-08-01

## Scope and changed files

Only the authorized files were changed:

- `scripts/wave-c3-governed-write-acceptance.mjs`
- `docs/execution/2026-08-01-wave-c3f-real-profile-snapshot-harness-fix-report.md`

The harness now captures `realProfileAfter` and calls
`assertPathSnapshotUnchanged(realProfileBefore, realProfileAfter, "real
LocalAppData WebView2 profile")`. It does not weaken, skip, catch, or change
the profile immutability check. No product code, shared process/profile helper,
scenario order, timeout, cleanup behavior, profile directory, package, roadmap,
or checkpoint was changed.

## Terminal records

Controller RED (before repair):

```text
node --test --test-concurrency=1 apps/desktop/tests/e2e/c3-governed-write.spec.ts
exit 1 after 247.9 seconds
AssertionError: undefined changed during isolated acceptance.
actual: []
expected: <path>
at assertPathSnapshotUnchanged (...desktop-profile.mjs:40)
```

Worker GREEN (after repair):

```text
node --test --test-concurrency=1 apps/desktop/tests/e2e/c3-governed-write.spec.ts
exit 0; 1 passed, 0 failed; duration_ms 243157.8374
Wave C3-F proves governed write reject/approve/modify/drift/cancel/isolation/restart over real Tauri WebView2
```

The successful terminal record is `status: "passed"` at the real
`tauri-webview` IPC boundary. The final `snapshotPath(realProfile)` followed by
the three-argument immutability assertion completed without error, so the real
LocalAppData WebView2 profile was unchanged.

## Scenario results

| Scenario | Result |
| --- | --- |
| reject-create | passed; target absent after rejection |
| approve-create | passed; expected SHA persisted |
| approve-modify | passed; expected SHA persisted and file opened |
| base-drift | passed; drifted base remained unchanged |
| cancellation | passed; target absent |
| conflict-replay | passed; target absent and one proposal only |
| two-projects | passed; `isolation: "passed"`, A persisted and B remained absent |
| forced-restart | passed; `interrupted: true`, `autoApply: false`, target absent |

Process cleanup passed. The terminal record reported residual Pillow processes
`0` and residual port listeners `0`.

## Verification

```text
node --check scripts/wave-c3-governed-write-acceptance.mjs
exit 0

node --test scripts/lib/desktop-process-safety.test.mjs
29 passed, 0 failed; duration_ms 90.5041

node --test --test-concurrency=1 apps/desktop/tests/e2e/c3-governed-write.spec.ts
1 passed, 0 failed; duration_ms 243157.8374

git diff --check -- scripts/wave-c3-governed-write-acceptance.mjs docs/execution/2026-08-01-wave-c3f-real-profile-snapshot-harness-fix-report.md
exit 0

git diff --cached --name-only
(empty)
```

HEAD remained `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`; no files were staged
or committed. The checkout was already dirty before this worker began and its
unrelated changes were preserved.

## Remaining risks

The focused E2E is worker development evidence only. Per the brief, controller
inspection and an independent rerun remain the acceptance gate. Keeper boot
and fallback session claim both returned HTTP 400 before implementation; this
did not affect the local scoped test run, but the Keeper session should be
rechecked by the controller if ingest/coordination requires it.
