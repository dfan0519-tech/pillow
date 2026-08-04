# Wave C3-F Product Fix Brief — Run Event Schema Alignment

Owner: Grok3 storage worker; controller owns integration and acceptance

Worker session: `<keeper-session>`

Controller session: `<keeper-session>`

## Confirmed Root Cause

The C3-F real Tauri path deterministically fails before the approval bar:
`RuntimeEventKind` and the conversation pipeline emit
`write_proposal_requested` / `write_proposal_resolved`, while the durable
`run_events.kind` CHECK created by migration 0002 rejects both values.
Migration 0005 adds the write proposal ledger but does not upgrade this table.

This is a product storage migration defect, not an acceptance-harness defect.

## Goal

Align fresh and already-existing Pillow databases with the frozen C3 event
contract without losing any project, conversation, run or event data. The
migration must be transactional, idempotent and safe with foreign keys enabled.
Unknown event kinds must remain rejected.

## TDD Contract

Before production changes, add a regression that builds a legacy 0001-0005
database with the old `run_events` CHECK, seeds at least one existing event,
and demonstrates that the two C3 kinds are rejected. Record the expected RED.

Then implement the smallest migration that proves all of the following:

1. a fresh database accepts both C3 kinds;
2. an existing legacy database is upgraded and accepts both kinds;
3. every pre-existing row and its sequence/payload/timestamps survive exactly;
4. the unique `(run_id, sequence)` rule, primary key, run foreign key and
   `idx_run_events_run_sequence` remain effective;
5. an arbitrary unregistered kind is still rejected;
6. applying migrations twice is a no-op with identical rows/schema;
7. a migration failure rolls back rather than leaving a half-rebuilt table.

Do not solve only the fresh-database case by editing 0002. Existing C2 user
databases are part of the contract. If using a schema marker or conditional
rebuild, keep it generic and deterministic; never infer success only from a
loose substring when a stronger invariant is available.

## File Whitelist

- `crates/storage-sqlite/migrations/0002_conversations.sql`
- one new `crates/storage-sqlite/migrations/0006_*.sql` if needed
- `crates/storage-sqlite/src/migrations.rs`
- `crates/storage-sqlite/tests/migrations.rs`
- `crates/storage-sqlite/tests/conversation_events.rs` only if a repository-level
  regression is necessary
- `docs/execution/2026-08-01-wave-c3f-run-events-schema-fix-report.md`

Do not change C3-F harness files, renderer, native commands, runtime, adapter,
policy, executor or write ledger logic.

## Required Gates

- targeted RED then GREEN migration regression;
- `cargo test -p keeper-storage-sqlite --tests -- --test-threads=1`;
- `cargo check --workspace --all-targets`;
- `cargo clippy -p keeper-storage-sqlite --all-targets -- -D warnings`;
- `cargo fmt --all -- --check`;
- `git diff --check` on the whitelist;
- staging empty and HEAD unchanged.

## Completion

Report the exact RED/green evidence, migration strategy, preservation checks,
changed files and residual risks. End with `COMPLETION: DONE` or
`COMPLETION: BLOCKED`. Do not reset, restore, stage or commit. Worker completion
is not controller acceptance.
