# Wave C3 Codex2 incomplete-descendant age development report

Status: DONE_WITH_CONCERNS

Date: 2026-08-02

## Scope and repair

Only the three authorized files changed:

- `scripts/lib/desktop-process.mjs`
- `scripts/lib/desktop-process-safety.test.mjs`
- `docs/execution/2026-08-02-wave-c3-codex2-incomplete-descendant-age-development-report.md`

`discoverDescendants` now independently normalizes an incomplete candidate's
`creationTicks`. When that tick is present and valid, the candidate is tracked
only if it is strictly later than its verified parent's creation tick. Older
and equal-tick candidates are ignored, so a recycled numeric parent PID cannot
turn an older protected process into a tracked descendant.

Missing or invalid ticks remain fail-closed: the candidate continues to be
recorded as `identity-incomplete` and blocks cleanup. No system-process name
allowlist or generic ignore mechanism was added.

## RED/GREEN evidence

RED: the focused safety suite exited 1 with 39/41 passing. Both older and
equal-tick incomplete candidates were incorrectly recorded as
`identity-incomplete`.

GREEN: the suite exits 0 with 41/41 passing. New coverage proves older/equal
candidates are ignored, while newer and missing/invalid-tick candidates remain
tracked and block cleanup. Existing VCTIP, root identity, PID reuse, and
Pillow/fake ACP/WebView fail-closed cases remain green.

## Verification and boundary

- `node --test scripts/lib/desktop-process-safety.test.mjs` — exit 0; 41
  passed, 0 failed.
- `node --check scripts/lib/desktop-process.mjs` — exit 0.

No real desktop E2E, stage, commit, reset, restore, or clean operation was
performed. The controller retains source review and the next real C3 run.

## Remaining risk

The repair is covered with deterministic synthetic snapshots, including the
system-start-versus-transient-parent ordering class, but not yet by the
controller's live CIM/C3 acceptance. A missing or unreadable creation tick is
intentionally still a cleanup blocker.
