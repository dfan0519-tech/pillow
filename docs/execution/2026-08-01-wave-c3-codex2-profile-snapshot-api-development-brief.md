# Wave C3 Codex2 profile-snapshot API development brief

Date: 2026-08-01

## Ownership

- Worker identity: Keeper `codex2`.
- Model: `gpt-5.6-terra`, reasoning `high`, `fastMode=false`.
- Controller: Codex Desktop; controller owns final source review, focused reruns,
  real desktop acceptance, and all repository-wide gates.
- Repository: `this repository`.
- Frozen reference: `legacy Keeper (out of tree)`; read-only, never modify it.

## Objective

Replace the error-prone positional real-profile snapshot assertion at the C3
acceptance call site with one small object-argument helper that captures the
after snapshot itself. This follows a real harness failure where a path string
was accidentally passed as the `after` snapshot and the label became
`undefined`.

Add focused Node tests using temporary directories. Required behaviors:

1. unchanged path passes;
2. a changed file rejects and includes the caller-provided label;
3. an invalid or missing `before` snapshot fails early with a useful contract
   error;
4. the C3 orchestrator calls the helper with named `{ root, before, label }`
   fields;
5. the profile location, scenario order, timeouts, cleanup, and privacy behavior
   remain unchanged.

Prefer a narrow exported helper such as
`assertPathUnchangedSince({ root, before, label })`. Do not refactor unrelated
profile or process code.

## Writable allowlist

- `scripts/lib/desktop-profile.mjs`
- `scripts/lib/desktop-profile.test.mjs`
- `scripts/wave-c3-governed-write-acceptance.mjs`
- `docs/execution/2026-08-01-wave-c3-codex2-profile-snapshot-api-development-report.md`

Everything else is read-only. Do not stage, commit, reset, restore, clean, run
the real desktop E2E, or modify product code.

## Required checks

1. `node --test scripts/lib/desktop-profile.test.mjs`
2. `node --test scripts/lib/desktop-process-safety.test.mjs`
3. `node --check scripts/wave-c3-governed-write-acceptance.mjs`
4. `git diff --check -- scripts/lib/desktop-profile.mjs scripts/lib/desktop-profile.test.mjs scripts/wave-c3-governed-write-acceptance.mjs docs/execution/2026-08-01-wave-c3-codex2-profile-snapshot-api-development-report.md`

The controller will independently rerun these checks and the real C3 desktop
suite after reviewing the diff.

## Completion contract

Write the report in the allowlisted path and return one completion signal with:

- status: `DONE`, `DONE_WITH_CONCERNS`, or `BLOCKED`;
- exact changed files and API contract;
- test names/counts and exit status;
- remaining risks;
- confirmation that staging is empty and no owned process remains.
