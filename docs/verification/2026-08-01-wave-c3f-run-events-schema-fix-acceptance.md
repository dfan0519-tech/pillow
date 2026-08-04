# Pillow Wave C3-F Run-Event Schema Fix Acceptance

Date: 2026-08-01

Verdict: ACCEPT

Scope: align the durable `run_events.kind` constraint with the already
accepted C3 write-proposal event kinds, including safe upgrade of existing
databases and preservation of all prior event rows and constraints.

## Controller Findings Closed

The first worker delivery correctly repaired the schema but was rejected
because it also published `apply_migrations` from the storage crate solely for
test convenience. That expanded a production API outside the brief and was
unnecessary.

The corrected delivery:

- adds `write_proposal_requested` and `write_proposal_resolved` to the fresh
  database constraint;
- conditionally rebuilds legacy `run_events` tables in a transaction, copies
  the exact persisted columns and recreates the sequence index;
- probes the actual constraint behavior instead of parsing
  `sqlite_master` text;
- preserves primary-key, unique-sequence, foreign-key and unknown-kind
  rejection behavior;
- proves rollback and idempotent re-open behavior;
- reaches the migration through `SqliteConversationRepository::open`, without
  adding a public migration API.

## Fresh Controller Evidence

- Focused migration suite: 11/11 passed.
- Complete `keeper-storage-sqlite` test suites passed: unit 2,
  conversation-event suites 14, conversation repository 8, migrations 11,
  project repository 3, resource recents 5, tool calls 8 and write proposals
  25.
- `cargo check --workspace --all-targets`: passed.
- `cargo clippy -p keeper-storage-sqlite --all-targets -- -D warnings`:
  passed.
- `rustfmt --edition 2021 --check` on the changed migration Rust sources:
  passed.
- Whitelist `git diff --check`: passed.
- HEAD remains `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`; staging is empty.

## Boundary

This acceptance closes only the storage-schema blocker discovered by the
first C3-F real desktop run. C3-F itself remains active until the controller
accepts fresh real Tauri/WebView2 evidence for every governed-write scenario,
the complete desktop E2E suite and the remaining repository gates. It does
not authorize staging, commit, terminal/Git product work or runtime expansion.
