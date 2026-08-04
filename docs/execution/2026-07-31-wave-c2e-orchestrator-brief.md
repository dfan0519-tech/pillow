# Wave C2-E Orchestrator Worker Brief

Read first and treat as the exact requirements:
`docs/superpowers/plans/2026-07-31-c2e-isolation-restart-desktop-acceptance.md`.

## Role

You are grok1. Own the real Tauri lifecycle orchestrator and outer Node E2E.
The activity and resource helpers are parallel-owned imports; do not edit
their files.

## Exclusive write whitelist

- `scripts/wave-c2-resource-acceptance.mjs`
- `apps/desktop/tests/e2e/c2-resource-workbench.spec.ts`
- `apps/desktop/package.json`
- `docs/execution/2026-07-31-wave-c2e-orchestrator-report.md`

Do not edit any other path. In particular, do not edit Pillow production Rust,
React, gateways, shared process helpers, the parent plan, or the final
acceptance document.

## Required behavior

- First boot Keeper as a worker for project `pillow`, task
  `wave-c2e-orchestrator`.
- Preserve the dirty tree; no stage, commit, reset, restore, or cleanup outside
  the harness-owned temporary directory.
- Import the helper signatures frozen in the plan verbatim.
- Reuse existing desktop process/profile supervision instead of duplicating
  kill logic.
- Use a unique CDP port and a real Tauri WebView2 page.
- Register both projects through existing real Tauri/SQLite paths.
- Normal window close is required; forced cleanup must turn the run red.
- The E2E wrapper asserts the exact completion record defined in the plan.
- Add the test to the existing serial `test:e2e` command without removing or
  reordering existing tests.

## Focused tests

Run the new Node test. If parallel helper files are not present yet, prove the
orchestrator imports/parses and report the integration dependency rather than
editing helper files. Also run `git diff --check` on the whitelist.

## Completion contract

Write the report file with:

- changed files;
- exact test commands and results;
- process/temporary-directory cleanup behavior;
- residual risks;
- `DONE`, `DONE_WITH_CONCERNS`, or `BLOCKED`.

Return only the status, one-line test summary, concerns, and Keeper completion
message. Worker completion is not controller acceptance.
