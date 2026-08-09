# C4 D3AR2 Grok2 brief — process event durability and session vocabulary

Read this file first. It is the complete task contract.

You are the actual Grok CLI `grok4` worker selected by the live lease. Repair only the rejected D3AR
durable event seam. Boot Keeper as a `pillow` worker for task
`c4-d3ar2-process-event-durability`. Work only in `this repository`; Keeper is
read-only. Preserve HEAD `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`,
empty staging and unrelated dirty work. No stage, commit, reset, restore,
clean, package or publish.

## Rejection being repaired

Agent protocol now defines four fixed process event kinds, but
`kernel-session::RuntimeEventKind` cannot parse/serialize them and the live
SQLite `run_events.kind` CHECK rejects them. A typed payload that cannot enter
the authoritative timeline is not a completed event contract. Full
agent-protocol Clippy also has one `too_many_arguments` failure in
`tests/tool_contract.rs` that must be corrected without suppressing the lint.

## Required behavior

1. Add the exact existing protocol names to kernel-session:
   `process_proposal_requested`, `process_proposal_decided`,
   `process_started`, `process_terminal_outcome`. Update enum, `as_str`,
   `parse`, exhaustive constants/tests and any status derivation. These kinds
   are non-terminal at the run level.
2. Add a new idempotent transactional SQLite migration after 0006 that rebuilds
   `run_events` only when any of the four process kinds is rejected. The final
   CHECK must retain every existing B1/C3 kind and add all four process kinds.
3. Migration must preserve rows, foreign keys, unique constraints and index;
   work from both a legacy pre-0006 database and a current 0006/0007 database;
   be idempotent; probe postconditions before commit; and roll back completely
   on injected mid-rebuild failure.
4. Repository append/load timeline tests must persist all four kinds in order
   and preserve replay/conflict/status semantics.
5. Fix the `process_run_output_json` Clippy failure in
   `agent-protocol/tests/tool_contract.rs` structurally (fixture/options struct
   or equivalent). Do not add `allow(clippy::too_many_arguments)` and do not
   weaken assertions.

## Exact write allowlist

- `crates/kernel-session/src/domain.rs`
- `crates/storage-sqlite/migrations/0008_run_events_process_kinds.sql`
- `crates/storage-sqlite/src/migrations.rs`
- `crates/storage-sqlite/tests/migrations.rs`
- `crates/storage-sqlite/tests/conversation_events.rs`
- `crates/agent-protocol/tests/tool_contract.rs`
- `docs/execution/2026-08-04-wave-c4-d3ar2-grok4-process-event-durability-report.md`

Do not touch agent-protocol product sources, process/tool executor,
runtime-host, policy, Tauri/TS, renderer, Git, root manifests/lockfile,
roadmap/progress or the parallel live-output worker's files.

## Required tests and gates

Use TDD. Run:

```text
cargo test -p keeper-session --all-targets
cargo test -p keeper-storage-sqlite --all-targets
cargo test -p keeper-agent-protocol --all-targets
cargo check -p keeper-session -p keeper-storage-sqlite -p keeper-agent-protocol --all-targets
cargo clippy -p keeper-session -p keeper-storage-sqlite -p keeper-agent-protocol --all-targets -- -D warnings
cargo fmt --all --check
git diff --check -- <allowlisted source/test/migration paths>
git diff --cached --name-only
git rev-parse HEAD
```

Write the exact report path with exact changed files, migration upgrade and
rollback evidence, commands, risks, HEAD/staging and residual-process evidence.
End with `COMPLETION: complete` only if every requirement is met; otherwise
report `BLOCKED` or `DONE_WITH_CONCERNS`. Controller acceptance is separate.
