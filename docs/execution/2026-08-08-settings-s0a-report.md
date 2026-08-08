# S0a Report: Settings Layer Storage + Resolution Service

Date: 2026-08-08  
Worker: Grok (S0a)  
Controller: Claude  
Status: **SETTINGS-S0A-COMPLETE**

## 0. Summary

SQLite is now the durable authority for the three S0 non-secret runtime
settings. Environment variables remain an allow-listed override only. A
resolution service returns both the effective value and **which layer won**,
so S0b UI can surface “overridden by environment.” Invalid stored adapter
enums fail closed with `invalid_runtime_selection` and **never** fall back to
Fake.

No UI, no `runtime_composition.rs` wiring, no credentials.

## 1. Schema (`0009_settings.sql`)

Table: `settings_entries`

| Column | Role |
| --- | --- |
| `setting_id` | Primary key (UUID text) |
| `scope` | `global` \| `project` (CHECK) |
| `project_id` | NULL for global; FK → `projects(id)` for project |
| `key` | Dotted namespace (e.g. `runtime.adapter`) |
| `value_kind` | Host-validated kind string (`enum`, `path` for S0) |
| `value_json` | JSON scalar payload (validated in Rust) |
| `updated_at` | RFC3339 |
| `updated_by` | `user` / `system` / `migration` (length-checked in SQL) |

**Scope shape CHECK (SQL only):**

- `global` ⇔ `project_id IS NULL`
- `project` ⇔ `project_id IS NOT NULL`

**Uniqueness (SQLite-safe partial indexes):**

- `idx_settings_entries_global_key` on `(key) WHERE scope = 'global'`
- `idx_settings_entries_project_key` on `(project_id, key) WHERE scope = 'project'`

Reason: SQLite treats `NULL` as distinct under ordinary `UNIQUE`, so a plain
`UNIQUE(scope, project_id, key)` would allow duplicate global rows.

**What SQL deliberately does *not* do:** enum value lists, path shape, or
known-key allow-lists. Those live in Rust (ADR-0003 / design §5.6).

**No secret columns.** Explicit test walks `PRAGMA table_info` and rejects
secret-like substrings; also asserts no `%secret%` tables.

## 2. S0 key space

| Key | Kind | Env override | Default |
| --- | --- | --- | --- |
| `runtime.adapter` | enum | `PILLOW_RUNTIME` | `grok-acp` |
| `runtime.grok.executable` | path | `PILLOW_GROK_EXECUTABLE` | unset (`None`) |
| `runtime.grok.profile` | path | `PILLOW_GROK_PROFILE` | unset (`None`) |

Adapter parse mirrors desktop `RuntimeSelection` labels:

- `""` / `grok` / `grok-acp` / `acp` → `GrokAcp`
- `fake` / `fixture` → `Fake`
- anything else → **error** (`invalid_runtime_selection`)

## 3. Repository

`SqliteSettingsRepository` in `crates/storage-sqlite/src/settings.rs`:

- `open` → migrations + `PRAGMA foreign_keys = ON`
- `get` / `list` by scope
- `upsert` (typed validation for the three S0 keys)
- `upsert_runtime_adapter` / `upsert_path` convenience writers
- `delete` one key at scope
- `delete_project_scope` all keys for a project

Error mapping follows the crate style (`SettingsError::Sqlite`,
`LockPoisoned`, coded domain errors via `SettingsError::code()`).

## 4. Resolution service (same crate)

Precedence (documented + tested):

```text
env(allow-list) → project → global → default
```

API:

- `resolve_runtime(project_id, env: &HashMap) → ResolvedRuntimeSettings`
- Each field is `ResolvedSetting<T> { value, source }`
- `SettingSource`: `Env` | `Project` | `Global` | `Default` (`as_str()` for UI)

Env is injected as a map so tests do not mutate process environment.
Production helper: `process_env_map()` reads only the three allow-listed names.

**Fail-closed:** if a project (or global, or env) layer holds an illegal
adapter label, resolve returns `SettingsError::InvalidRuntimeSelection` and
**stops** — it does not skip to the next layer or invent Fake.

Resolution lives **inside** `keeper-storage-sqlite` (whitelist allowed; no new
crate). S0b can re-export or call this from desktop composition.

## 5. Tests (six required cases)

| # | Requirement | Coverage |
| --- | --- | --- |
| 1 | Migration idempotent | `migration_0009_is_idempotent_and_creates_settings_entries` — double open, columns, partial indexes, scope CHECK |
| 2 | Each precedence layer wins + source | `precedence_each_layer_wins_once_for_adapter` + dedicated default/global/project/env tests |
| 3 | Project overrides global | `precedence_project_wins_over_global` |
| 4 | Restart recovery | `settings_survive_repository_reopen` — write, drop, reopen, resolve |
| 5 | Illegal enum fail-closed, no Fake | `illegal_stored_adapter_fails_closed_never_fake`, project-scoped illegal, illegal env |
| 6 | No secret fields | `settings_schema_has_no_secret_fields` — column name + table name assertions |

Extra: upsert/delete/list, delete_project_scope, unknown key rejection.

Files:

- `crates/storage-sqlite/tests/settings_repository.rs` (14 tests)
- unit parse tests in `settings.rs`

## 6. Gate results

| Command | Exit code |
| --- | --- |
| `node scripts/run-worker-gate.mjs` | **0** (`PILLOW_WORKER_GATE: OK`) |
| Includes: workspace, secrets, `cargo fmt --check`, pnpm typecheck/test, full cargo test workspace, clippy `-D warnings`, ACP protocol drift | all green |

Focused pre-gate: `cargo test -p keeper-storage-sqlite --test settings_repository` → 14 passed.

## 7. Files touched (whitelist only)

- `crates/storage-sqlite/migrations/0009_settings.sql` **(new)**
- `crates/storage-sqlite/src/migrations.rs` (wire 0009)
- `crates/storage-sqlite/src/settings.rs` **(new)**
- `crates/storage-sqlite/src/lib.rs` (mod + re-exports)
- `crates/storage-sqlite/tests/settings_repository.rs` **(new)**
- `docs/execution/2026-08-08-settings-s0a-report.md` **(this report)**

**Not modified (hard exclusion):**

- `apps/desktop/**` (including `runtime_composition.rs`)
- `crates/runtime-host/**`
- `crates/kernel-policy/**`
- `scripts/**`
- any other crate’s `Cargo.toml`
- no new dependencies in `storage-sqlite/Cargo.toml`
- no git add/commit/stash/reset
- no desktop E2E / acceptance scripts

## 8. Residual risks / open items for S0b

1. **Composition not wired.** Desktop still reads env-only in
   `runtime_composition.rs`. S0b must call `resolve_runtime` (+
   `process_env_map`) and surface `SettingSource::Env` in the UI.
2. **Path existence / discovery** still belongs to composition (executable on
   PATH, profile home). S0a only stores and resolves path *strings*.
3. **Unknown keys rejected on write** by design for S0; future UI prefs need an
   explicit key-space expansion.
4. **Project FK** requires the project row to exist before project-scoped
   upsert; no `ON DELETE CASCADE` — orphan cleanup is `delete_project_scope`
   (callers must invoke on project removal if that path exists later).
5. Keeper bridge boot returned HTTP 400 in this environment; work proceeded
   per task contract without blocking on session recording.

## 9. Completion token

**SETTINGS-S0A-COMPLETE**
