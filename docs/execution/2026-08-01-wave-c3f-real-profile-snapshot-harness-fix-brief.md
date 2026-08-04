# Wave C3-F real-profile snapshot harness fix brief

Owner: one harness implementation worker

Controller: Codex Desktop, session `<keeper-session>`

Date: 2026-08-01

Status: authorized conditional repair after controller-classified
`HARNESS_ASSERTION_OR_DRIVER` failure

## Objective

Repair the final real-WebView-profile immutability assertion in the C3-F
orchestrator without touching product code, shared process-safety authority or
scenario semantics.

## Controller RED evidence

The controller ran:

```powershell
node --test --test-concurrency=1 apps/desktop/tests/e2e/c3-governed-write.spec.ts
```

Result: exit `1` after 247.9 seconds. All eight product scenarios completed and
the terminal record reported:

- reject-create: passed;
- approve-create: passed;
- approve-modify: passed;
- base-drift: passed;
- cancellation: passed;
- conflict-replay: passed;
- two-projects: `isolation: passed`;
- forced-restart: `interrupted: true`, `autoApply: false`;
- residual Pillow processes: `0`;
- residual port listeners: `0`.

The only failure occurred after those scenarios:

```text
AssertionError: undefined changed during isolated acceptance.
+ actual - expected

+ []
- '<path>'

at assertPathSnapshotUnchanged (scripts/lib/desktop-profile.mjs:40)
at scripts/wave-c3-governed-write-acceptance.mjs:1681
```

## Root cause established by controller

`scripts/wave-c3-governed-write-acceptance.mjs` captures:

```js
const realProfile = defaultWebViewProfilePath();
const realProfileBefore = await snapshotPath(realProfile);
```

but its final check currently calls:

```js
await assertPathSnapshotUnchanged(realProfile, realProfileBefore);
```

This passes the profile path string as `before`, passes the before-array as
`after`, omits the label, and never captures an after snapshot. That exactly
explains the `undefined`, `[]` and sanitized path values in the assertion.

Working C2/native/smoke examples use this contract:

```js
const realProfileAfter = await snapshotPath(realProfile);
assertPathSnapshotUnchanged(
  realProfileBefore,
  realProfileAfter,
  "real LocalAppData WebView2 profile",
);
```

## Write allowlist

- `scripts/wave-c3-governed-write-acceptance.mjs`
- `docs/execution/2026-08-01-wave-c3f-real-profile-snapshot-harness-fix-report.md`

Everything else is read-only. In particular, do not edit:

- `scripts/lib/desktop-process.mjs`;
- `scripts/lib/desktop-profile.mjs`;
- `scripts/lib/c3-governed-write-acceptance.mjs`;
- `apps/desktop/**`;
- `crates/**`;
- package manifests, roadmap, checkpoint or prior reports.

Do not stage or commit.

## Required implementation

Replace only the malformed final call with the working three-argument pattern:

```js
const realProfileAfter = await snapshotPath(realProfile);
assertPathSnapshotUnchanged(
  realProfileBefore,
  realProfileAfter,
  "real LocalAppData WebView2 profile",
);
```

Do not weaken, skip, catch or sanitize away the immutability assertion. Do not
change scenario order, timeouts, cleanup behavior, profile directories or
terminal-record requirements.

## Verification

Run in this order:

```powershell
node --check scripts/wave-c3-governed-write-acceptance.mjs
node --test scripts/lib/desktop-process-safety.test.mjs
node --test --test-concurrency=1 apps/desktop/tests/e2e/c3-governed-write.spec.ts
git diff --check -- scripts/wave-c3-governed-write-acceptance.mjs docs/execution/2026-08-01-wave-c3f-real-profile-snapshot-harness-fix-report.md
git diff --cached --name-only
```

The worker's E2E run is development evidence only. The controller will inspect
the diff and rerun the same focused E2E personally before accepting C3-F.

## Completion contract

Write the report at:

`docs/execution/2026-08-01-wave-c3f-real-profile-snapshot-harness-fix-report.md`

The report must include:

- exact changed files;
- controller RED and worker GREEN terminal records;
- all eight scenario results;
- real-profile immutability result;
- process cleanup and residual counts;
- safety test and syntax results;
- staging and HEAD state;
- remaining risks;
- one completion status: `DONE`, `DONE_WITH_CONCERNS`, `BLOCKED` or
  `NEEDS_CONTEXT`.

Send the controller a short completion/blocker signal when finished. Do not
create a periodic monitor.
