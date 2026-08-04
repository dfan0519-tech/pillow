# Wave C3-F Product Fix Report — Run Event Schema Alignment

Owner: Grok3 storage worker
Worker session (assigned): `<keeper-session>`
Worker bridge session: `<keeper-session>` (agent `grok-c3f-schema`)
Controller session: `<keeper-session>`
Correction worker session: `<keeper-session>`
Correction bridge session: `<keeper-session>` (agent `grok-c3f-schema-fix`)
Lease (original): `<keeper-lease>` on grok3
Lease (correction): `<keeper-lease>` on grok3
Date: 2026-08-01
HEAD: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
Staging: empty
Status: **DONE** (worker completion; not controller acceptance)

## Confirmed defect

C3-F real Tauri path failed before the approval bar because
`RuntimeEventKind` / conversation pipeline emit
`write_proposal_requested` and `write_proposal_resolved`, while
`run_events.kind` CHECK from migration 0002 rejected both. Migration 0005
added the write-proposal ledger tables but did not widen the event kind
CHECK. This is a product storage migration defect, not a harness defect.

## Controller rejection (correction trigger)

Controller rejected the first delivery because it modified
`crates/storage-sqlite/src/lib.rs` outside the original product whitelist to
publish `apply_migrations`. That expanded the production crate API solely for
integration-test convenience and was unnecessary: repository `open` methods
already exercise the real migration path.

Governing brief:
`docs/execution/2026-08-01-wave-c3f-run-events-schema-controller-rejection-fix-brief.md`

## Correction (this pass)

1. Removed only the unauthorized `apply_migrations` public re-export and its
   doc comment from `src/lib.rs`. All unrelated accepted dirty-tree lines in
   that file were preserved.
2. Rewrote the new migration tests to trigger production migration through
   `SqliteConversationRepository::open(&db)`.
3. Close/drop setup connections before repository open; reopen a direct test
   connection afterward for SQL assertions, with `PRAGMA foreign_keys = ON`.
4. Preserved RED proof, fresh/legacy/data-preservation/constraint/
   idempotence/rollback coverage without weakening assertions.
5. Did not redesign the accepted candidate migration (0002/0006/migrations.rs).

Helpers added in the test file only:

- `apply_via_repository_open` — opens `SqliteConversationRepository`, drops it
- `reopen_with_foreign_keys` — reopens rusqlite with FK enabled for assertions

Test rename for clarity (same coverage):

- `apply_migrations_reaches_write_proposal_kind_support`
  → `repository_open_reaches_write_proposal_kind_support`

## TDD evidence

### RED (before production migration; original delivery)

Regression suite added first. With migrations still only applying 0001–0005
(no kind upgrade):

```text
cargo test -p keeper-storage-sqlite --test migrations -- --test-threads=1
```

| Test | Result | Meaning |
| --- | --- | --- |
| `legacy_run_events_check_rejects_write_proposal_kinds` | **ok** | Documents historical CHECK reject |
| `fresh_database_accepts_write_proposal_kinds` | **FAILED** | CHECK reject on fresh DB |
| `legacy_database_upgrade_preserves_rows_and_accepts_write_proposal_kinds` | **FAILED** | No upgrade path |
| `run_events_kind_upgrade_is_idempotent` | **FAILED** | C3 kinds still rejected |
| `run_events_kind_upgrade_rolls_back_on_failure` | **FAILED** | Post-rollback production apply still rejects |
| `apply_migrations_reaches_write_proposal_kind_support` | **FAILED** | Full apply still rejects |

Exact RED failure signature (representative):

```text
CHECK constraint failed: kind IN (
  'run_started', …, 'artifact_produced',
  'turn_completed', 'turn_failed', 'run_cancelled', 'heartbeat'
)
```

(`write_proposal_requested` / `write_proposal_resolved` absent.)

RED score: **6 passed; 5 failed**.

### GREEN (after migration + API correction)

```text
cargo test -p keeper-storage-sqlite --test migrations -- --test-threads=1
→ 11 passed; 0 failed
```

Production migration path is exercised exclusively via
`SqliteConversationRepository::open` (no public `apply_migrations` API).

## Migration strategy (unchanged by correction)

1. **Fresh installs** — update
   `crates/storage-sqlite/migrations/0002_conversations.sql` so
   `run_events.kind` CHECK includes:
   - `write_proposal_requested`
   - `write_proposal_resolved`
   (placed after `artifact_produced`, matching domain order).

2. **Existing C2/C3-B databases** — new
   `crates/storage-sqlite/migrations/0006_run_events_write_kinds.sql`:
   - `CREATE TABLE run_events__new` with expanded CHECK
   - `INSERT … SELECT` all existing rows (exact column copy)
   - `DROP TABLE run_events`
   - `ALTER TABLE run_events__new RENAME TO run_events`
   - recreate `idx_run_events_run_sequence`

3. **`migrations.rs` orchestration**
   - After 0001–0005, call conditional upgrade.
   - **Need-upgrade probe** uses a savepoint insert of each C3 kind and
     classifies `SQLITE_CONSTRAINT_CHECK` vs other constraint failures
     (FK/PK/UNIQUE). Stronger than scanning `sqlite_master` SQL text.
   - Rebuild runs inside `BEGIN IMMEDIATE` / `COMMIT`, with `ROLLBACK` on
     any batch or post-condition failure.
   - Post-condition re-probes both kinds before commit.
   - Second apply is a no-op when both kinds already pass CHECK.

4. **Public API** — **no new public export**. Repositories already call
   `migrations::apply` internally on `open`. Tests use
   `SqliteConversationRepository::open`.

## Contract coverage

| # | Requirement | Evidence |
| --- | ---: | --- |
| 1 | Fresh DB accepts both C3 kinds | `fresh_database_accepts_write_proposal_kinds` |
| 2 | Legacy 0001–0005 DB upgrades and accepts | `legacy_database_upgrade_preserves_rows_and_accepts_write_proposal_kinds` |
| 3 | Pre-existing rows/sequence/payload/timestamps survive exactly | Same test; two seeded events compared before/after |
| 4 | UNIQUE `(run_id, sequence)`, PK, run FK, index remain | `run_events_invariants_hold_after_kind_upgrade` |
| 5 | Unknown kind still rejected | Same test (`not_a_real_kind` → CHECK fail) |
| 6 | Double apply no-op (identical rows + schema SQL) | `run_events_kind_upgrade_is_idempotent` (two repository opens) |
| 7 | Mid-rebuild failure rolls back | `run_events_kind_upgrade_rolls_back_on_failure` (forced fail after DROP; seed + old SQL restored; successful production open still works) |

## Changed files

### Original schema delivery (accepted candidate; not re-touched this pass)

| Path | Change |
| --- | --- |
| `crates/storage-sqlite/migrations/0002_conversations.sql` | Add two C3 kinds to fresh `run_events.kind` CHECK |
| `crates/storage-sqlite/migrations/0006_run_events_write_kinds.sql` | **New** transactional rebuild SQL for existing DBs |
| `crates/storage-sqlite/src/migrations.rs` | Conditional probe + transactional 0006 apply |

### Correction whitelist (this pass only)

| Path | Change |
| --- | --- |
| `crates/storage-sqlite/src/lib.rs` | Remove unauthorized `apply_migrations` re-export + doc comment only |
| `crates/storage-sqlite/tests/migrations.rs` | Route production apply through `SqliteConversationRepository::open` |
| `docs/execution/2026-08-01-wave-c3f-run-events-schema-fix-report.md` | Rejection/correction + gate results |

Not modified this pass: migration SQL, `src/migrations.rs` implementation,
C3-F harness, renderer, native commands, runtime, adapter, policy, executor,
write ledger logic, old Keeper tree.

## Gate evidence (post-correction)

### Targeted migrations

```text
cargo test -p keeper-storage-sqlite --test migrations -- --test-threads=1
→ 11 passed; 0 failed
```

### Full storage package tests

```text
cargo test -p keeper-storage-sqlite --tests -- --test-threads=1
```

All suites green (including unit + integration):

| Suite | Result |
| --- | --- |
| unit (`keeper_storage_sqlite`) | 2 passed |
| `conversation_event_replay_status` | 1 passed |
| `conversation_event_status` | 7 passed |
| `conversation_events` | 6 passed |
| `conversation_repository` | 8 passed |
| `migrations` | 11 passed |
| `project_repository` | 3 passed |
| `resource_recent_repository` | 5 passed |
| `tool_call_repository` | 8 passed |
| `write_proposal_repository` | 25 passed |

### Workspace check

```text
cargo check --workspace --all-targets
→ Finished (CHECK_EXIT=0)
```

### Clippy

```text
cargo clippy -p keeper-storage-sqlite --all-targets -- -D warnings
→ Finished (CLIPPY_EXIT=0)
```

### rustfmt

- `rustfmt --edition 2021 --check crates/storage-sqlite/tests/migrations.rs`
  → exit 0
- `lib.rs` change is a two-line deletion only; no reformatting needed.
- Full-crate `rustfmt` on `src/lib.rs` fails to parse pre-existing dirty-tree
  `conversation.rs` let-chain syntax (edition tooling); outside this
  correction whitelist and not introduced here.
- No `storage-sqlite` test formatting drift after correction.

### git hygiene

```text
git rev-parse HEAD
→ 7b7f70f7360131cd1cfd984a8b19bdfb5725c21b

git diff --cached --name-only
→ (empty) STAGED_COUNT=0

git diff --check -- crates/storage-sqlite/src/lib.rs crates/storage-sqlite/tests/migrations.rs
→ DIFF_CHECK_EXIT=0 (LF/CRLF conversion warnings only)
```

Whitelist-only correction touch set:

- `crates/storage-sqlite/src/lib.rs` (remove public re-export)
- `crates/storage-sqlite/tests/migrations.rs` (repository-open path)
- `docs/execution/2026-08-01-wave-c3f-run-events-schema-fix-report.md`

No reset, restore, stage, commit, package, or publish. Accepted dirty tree
preserved.

## Residual risks

1. **C3-F real Tauri acceptance not re-run here** — schema fix unblocks
   `append_event` for write-proposal kinds; controller must re-run focused
   C3-F harness for approval-bar / scenario acceptance.
2. **In-flight processes holding a pre-upgrade DB handle** need reopen or
   restart so repository `open` re-applies migrations.
3. **Workspace `cargo fmt --all -- --check`** may remain red from accepted
   dirty-tree formatting outside this whitelist (e.g. adapter scripts /
   let-chain tooling). Not introduced by this correction.
4. Worker completion is **not** controller acceptance.

## Process hygiene

- No old Keeper writes
- No reset/restore
- No stage/commit
- No package/publish
- No harness / renderer / native / runtime / adapter / policy / executor edits
- No migration implementation redesign this pass
- HEAD unchanged; staging empty
- Exact correction whitelist obeyed

## Completion

**COMPLETION: DONE**
