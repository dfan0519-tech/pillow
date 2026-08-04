# C4 D3AR2 Grok4 report — process event durability and session vocabulary

Date: 2026-08-04
Worker: grok4 (Keeper session `<keeper-session>`)
Lease: `<keeper-lease>` (exact release target)
Task: `c4-d3ar2-process-event-durability`
Brief: `docs/execution/2026-08-04-wave-c4-d3ar2-grok2-process-event-durability-brief.md`
Status: **COMPLETION: complete** (worker completion; controller acceptance separate)

## Rejection repaired

Agent protocol defined four fixed process event kinds, but
`kernel-session::RuntimeEventKind` could not parse/serialize them and the live
SQLite `run_events.kind` CHECK rejected them. A typed payload that cannot enter
the authoritative timeline is not a completed event contract. Full
agent-protocol Clippy also failed on `too_many_arguments` in
`tests/tool_contract.rs` (`process_run_output_json`).

## Exact changed files (allowlist only)

| Path | Action |
| --- | --- |
| `crates/kernel-session/src/domain.rs` | Four process kinds; `ALL` (21); `is_terminal`; `status_for_kind`; `as_str`/`parse`; unit tests |
| `crates/storage-sqlite/migrations/0008_run_events_process_kinds.sql` | **New** — transactional rebuild CHECK with B1/C3 + four process kinds |
| `crates/storage-sqlite/src/migrations.rs` | Conditional 0008 upgrade after 0007; probe postconditions; full rollback on failure |
| `crates/storage-sqlite/tests/migrations.rs` | RED/GREEN process-kind migration suite (legacy + 0006/0007 + fresh + idempotent + rollback) |
| `crates/storage-sqlite/tests/conversation_events.rs` | Append/load all four process kinds; replay/conflict; non-terminal run status |
| `crates/agent-protocol/tests/tool_contract.rs` | Structural `ProcessRunOutputFixture` (no `allow` lint) |
| `docs/execution/2026-08-04-wave-c4-d3ar2-grok4-process-event-durability-report.md` | This report |

No stage, commit, reset, restore, clean, package, or publish. Unrelated dirty tree preserved.

## Domain vocabulary

Added exact protocol names (non-terminal at the run level):

- `process_proposal_requested`
- `process_proposal_decided`
- `process_started`
- `process_terminal_outcome`

`RuntimeEventKind::ALL` length is 21. Terminal kinds remain only
`turn_completed` / `turn_failed` / `run_cancelled`. Process kinds return
`None` from `status_for_kind`.

## Migration 0008 strategy

1. **Probe** live CHECK via savepoint insert for each process kind.
2. **Skip** rebuild when all four are already accepted (idempotent).
3. **Rebuild** under `BEGIN IMMEDIATE`: create `run_events__new` with full CHECK
   (all B1/C3 kinds + four process kinds) → copy rows → drop → rename →
   recreate `idx_run_events_run_sequence`.
4. **Postcondition** re-probes all four kinds before `COMMIT`; any failure
   `ROLLBACK`s completely.

Works from:

- legacy pre-0006 databases (after 0006 write-kind upgrade in the same `apply`)
- current 0006/0007 databases (write kinds present, process kinds rejected)
- fresh repository open (0002 write kinds → 0006 no-op → 0007 → 0008 upgrade)

## TDD / gates

### Commands and results

| Command | Result |
| --- | --- |
| `cargo test -p keeper-session --all-targets` | **PASS** (10 tests; process vocabulary unit tests green) |
| `cargo test -p keeper-storage-sqlite --all-targets` | **PASS** (all suites; process migration 7 + conversation process timeline 1 green) |
| `cargo test -p keeper-agent-protocol --all-targets` | **PASS** (lib 14 + process_event 4 + tool_contract 40 + security 18) |
| `cargo check -p keeper-session -p keeper-storage-sqlite -p keeper-agent-protocol --all-targets` | **PASS** |
| `cargo clippy -p keeper-session -p keeper-storage-sqlite -p keeper-agent-protocol --all-targets -- -D warnings` | **PASS** (0 warnings; no `allow(clippy::too_many_arguments)`) |
| `cargo fmt --all --check` | **PASS** (after allowlisted rustfmt) |
| `git diff --check -- <allowlisted paths>` | **PASS** (CRLF normalization warnings only) |
| `git diff --cached --name-only` | **empty** |
| `git rev-parse HEAD` | `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b` |

### Migration evidence map

| Test | Proves |
| --- | --- |
| `current_0006_0007_check_rejects_process_kinds` | RED baseline: 0006/0007 CHECK rejects all four |
| `fresh_database_accepts_process_kinds` | Full apply accepts process kinds; B1/C3 retained in SQL |
| `legacy_pre_0006_upgrade_preserves_rows_and_accepts_process_kinds` | Legacy path preserves rows |
| `current_0006_0007_upgrade_preserves_rows_and_accepts_process_kinds` | 0006/0007 path preserves rows; UNIQUE/FK/index hold |
| `run_events_process_kind_upgrade_is_idempotent` | Second open no-ops schema/rows |
| `run_events_process_kind_upgrade_rolls_back_on_failure` | Injected mid-rebuild failure restores table/rows/CHECK |
| `repository_open_reaches_process_kind_support` | Production open path only |
| `append_and_load_timeline_persists_all_four_process_kinds_in_order` | Repository append/load order; replay; conflict; run stays pending |

## HEAD / staging

- HEAD: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b` (preserved)
- Staging: empty
- Unrelated dirty work preserved (not staged)

## Residual processes

No worker-owned residual process authority after tests. Ambient IDE/shell may
exist; no PID/name cleanup performed. No package or publish.

## Residual risks (controller)

1. Desktop `status_for_kind` in `apps/desktop` is outside allowlist; wildcard
   already returns `None` for unknown kinds, so process kinds do not drive run
   status there either. Aligning that match arms explicitly is optional follow-up.
2. Fresh `0002_conversations.sql` still omits process kinds (same pattern as
   write kinds relying on 0006). 0008 upgrades live DBs; optional later 0002
   CHECK expansion for brand-new installs would skip the 0008 rebuild.
3. Controller source review and acceptance remain separate.

## Completion

`COMPLETION: complete`
