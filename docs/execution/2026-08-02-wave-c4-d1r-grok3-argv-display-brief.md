# C4-D1R Grok3 development brief — unambiguous argv display

Role: real Grok CLI slot `grok3`, implementation worker.

Boot Keeper as a worker for project `pillow`, task
`c4-d1r-argv-display` before editing. Read the three AGENTS files, the accepted
C4 design, Task 4 of the implementation plan, and the Grok3 D1 report. Preserve
all existing dirty work. Do not stage or commit.

## Controller finding to correct with TDD

`formatArgvDisplay` currently uses `argv.join(" ")`, which loses sealed
argument boundaries. For example `["a b", "c"]` is indistinguishable from
`["a", "b", "c"]`. Approval and review must display literal sealed arguments
unambiguously while execution continues to consume the original array.

Define one deterministic display-only encoding (JSON-string encoding per
argument is recommended), preserve empty strings, quotes, backslashes,
whitespace and Unicode, and never introduce shell parsing. Add tests proving
different argv arrays cannot render the same text for these boundary cases.

## Write allowlist

- `apps/desktop/src/features/execution/executionTypes.ts`
- `apps/desktop/tests/c4-execution-primitives.test.tsx`
- `docs/execution/2026-08-02-wave-c4-d1r-grok3-argv-display-report.md`

Do not change component layout or mount anything. Run the focused UI suite,
desktop typecheck, production UI build and scoped diff check. Report exact
tests, changed files, staging state, residual risk/processes, then send one
completion or blocker signal.
