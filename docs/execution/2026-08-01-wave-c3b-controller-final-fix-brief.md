# Wave C3-B Controller Final Narrow Fix

Owner: grok1 implementation worker
Date: 2026-08-01

Work only in `this repository`. Boot under controller session
`<keeper-session>`, read all AGENTS plus the C3 design and current
C3-B report, preserve the dirty tree, and do not stage/commit/reset.

## Allowed Files Only

- `crates/storage-sqlite/migrations/0005_write_proposals.sql`
- `crates/storage-sqlite/src/write_proposal.rs`
- `crates/storage-sqlite/tests/write_proposal_repository.rs`
- `crates/storage-sqlite/tests/migrations.rs`
- `crates/storage-sqlite/src/lib.rs`
- `docs/execution/2026-08-01-wave-c3b-storage-ledger-report.md`

## Corrections

1. Expand the fixed typed apply failure receipt vocabulary to exactly the
   apply-time C3 errors needed by the frozen design:
   `base_changed`, `path_rejected`, `write_failed`. Keep it closed and
   snake_case. Add serialization/round-trip and complete-apply tests for all
   three. Do not add approval, provider, terminal, Git or arbitrary error text.
2. Tighten migration checks so `write_proposals.result_json` is non-null only
   for `succeeded`/`failed` and must be null for every other state. Apply the
   equivalent state/result rule to transition rows: succeeded/failed require
   bounded valid JSON; every other transition requires null. Add direct SQL
   adversarial tests proving a pending/rejected/interrupted row cannot carry
   JSON and an apply terminal cannot omit it.
3. Update the C3-B report and remove the obsolete residual-risk claim that the
   failure vocabulary only supports `write_failed`.

Run the focused repository and migration suites, full storage tests,
`cargo check --workspace --all-targets`, fmt, diff check and staging-empty
check. End the report with `## COMPLETION: DONE` or `BLOCKED`.
