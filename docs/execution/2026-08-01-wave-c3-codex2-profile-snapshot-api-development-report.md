# Wave C3 Codex2 profile-snapshot API development report

Status: DONE

Date: 2026-08-01

## Scope and API contract

Only the brief allowlist was changed:

- `scripts/lib/desktop-profile.mjs`
- `scripts/lib/desktop-profile.test.mjs`
- `scripts/wave-c3-governed-write-acceptance.mjs`
- `docs/execution/2026-08-01-wave-c3-codex2-profile-snapshot-api-development-report.md`

`assertPathUnchangedSince({ root, before, label })` is an async helper. It
requires `before` to be the array returned by `snapshotPath`, captures the
after snapshot internally from `root`, then reports any drift with the supplied
`label`. The existing positional helper remains unchanged for other acceptance
scripts outside this worker's call site.

The C3 governed-write orchestrator now uses:

```js
await assertPathUnchangedSince({
  root: realProfile,
  before: realProfileBefore,
  label: "real LocalAppData WebView2 profile",
});
```

This preserves the profile location, scenario sequence, timeouts, cleanup, and
privacy behavior while preventing a positional `after` argument from being
mistakenly passed as a path string.

## Focused tests

- `node --test scripts/lib/desktop-profile.test.mjs` — exit 0; 4 passed, 0
  failed. Covers unchanged paths, changed-file label propagation, invalid or
  missing `before` snapshots, and the C3 named-argument call site.
- `node --test scripts/lib/desktop-process-safety.test.mjs` — exit 0; 29
  passed, 0 failed.
- `node --check scripts/wave-c3-governed-write-acceptance.mjs` — exit 0.

The profile API tests were added before implementation and initially failed
because `assertPathUnchangedSince` was not exported. They passed after the
minimal helper and call-site change.

## Boundaries and remaining risks

No real desktop E2E was run, as required by the brief. The controller retains
final source review, independent focused reruns, real desktop acceptance, and
repository-wide gates.

The checkout was already dirty before this scoped work; unrelated changes were
not modified. No staging, commit, reset, restore, or clean operation was run.
This worker launched no desktop process, and no worker-owned process remains.
