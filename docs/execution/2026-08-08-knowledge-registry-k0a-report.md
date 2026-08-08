# K0a Report: Knowledge Registry Storage Layer

Date: 2026-08-08  
Worker: Grok (K0a)  
Completion marker: **K0A-REGISTRY-COMPLETE**

## 1. One-line outcome

SQLite migration `0010` plus a typed repository let the system **mechanically record** each document’s role, lifecycle status, and multi-valued supersession links — without scanning `docs/`, without retirement, without UI.

## 2. Table structure and why

### 2.1 `knowledge_documents`

| Column | Why |
| --- | --- |
| `document_id` | Primary key (UUID text), stable identity independent of path renames later |
| `project_id` | FK → `projects(id)`; every durable object is project-scoped |
| `path` | Project-relative portable path; uniqueness with project |
| `role` | Inventory-aligned enum (`brief`…`other`, including first-class `plan`) |
| `status` | Expanded lifecycle; **DEFAULT `unknown`** (not `active`) |
| `content_digest` | Content fingerprint for change detection later |
| `updated_at` | RFC3339 wall clock of last registry write |
| `registered_by` | `controller` / `worker` / `lint` |
| `source_run_id` | Optional provenance to a run |

**Unique:** `(project_id, path)`.

**Why status is not `{active,superseded,archived}`:** inventory audit on 294 files showed briefs are *consumed*, reports are *evidence*, designs are *proposed/accepted*, and unclassified must stay *unknown*. Defaulting to `active` would lie.

### 2.2 `knowledge_document_links`

| Column | Why |
| --- | --- |
| `link_id` | Stable PK so multiple outbound edges coexist |
| `from_document_id` | Source document (NOT NULL) |
| `to_document_id` | **Nullable** — target not yet registered / unresolvable |
| `to_path` | **Nullable** — declared target path *or* free-text claim |
| `rel` | `supersedes` / `related` / `implements` |

**CHECK:** `to_document_id IS NOT NULL OR to_path IS NOT NULL`  
A link that points nowhere is garbage data.

**Why multi-valued (join table, not a single `supersedes` column):** the live handoff case is **1→2**:

- `docs/handoff/2026-08-08-pillow-controller-handoff.md`
  - → `…/2026-08-07-pillow-controller-handoff.md`
  - → `…/2026-08-08-round5-tech-debt-status.md`

A single-value column breaks on the first real registration.

**Why free-text `to_path` without `to_document_id` is allowed:** C3-E report claims to supersede “prior delivery status” with **no path**. Pathless claims must still be registerable.

### 2.3 SQL CHECK vs Rust validation

Same split as `0009_settings.sql`:

| Layer | Responsibility |
| --- | --- |
| SQL CHECK | Enum membership, non-empty shape, link endpoints not both NULL, lengths |
| Rust | Portable path via **SSoT**, `sha256:` digest format, empty-string normalization |

No secret columns anywhere.

## 3. Path validation uses SSoT (not a sixth copy)

### How

`crates/storage-sqlite/src/knowledge.rs` calls **only**:

```rust
use keeper_project::validate_portable_path;
// ...
validate_portable_path(path).map_err(|detail| KnowledgeError::InvalidPath { ... })
```

There is **no** local device-name table, no local component walk for document paths.

`storage-sqlite` already depends on `keeper-project`; **Cargo.toml was not modified**.

### Proof

1. **Unit test** `path_validation_uses_ssot_and_rejects_superscript_com` rejects `COM¹` (U+00B9) and also asserts `keeper_project::validate_portable_path("COM¹").is_err()` directly.
2. **Integration test** `path_validation_uses_ssot_rejects_superscript_com` registers `COM¹` and `docs/COM¹/note.md` through the repository API and expects `KnowledgeError::InvalidPath`.
3. Superscrit COM¹ is a **converged SSoT rule** (see portable-path converge report). A hand-rolled validator that only knew ASCII `COM1`–`COM9` would pass `COM¹` and fail this gate.

## 4. One-to-many supersession: implementation and verification

### Implementation

- Each edge is one row in `knowledge_document_links`.
- `SqliteKnowledgeRepository::add_link` inserts one row per call.
- `list_links_from` / `list_links_from_with_rel` return all outbound edges (1→N query).

No automatic status change when a `supersedes` link is written (ADR-0004 §5 / K0a hard rule). Status only changes via `register(status: Some(...))` or `update(status: Some(...))`.

### Verification

Test `one_to_two_supersedes_handoff_case` uses the **real audit paths**, writes two `supersedes` links from the current handoff, and asserts `list_links_from_with_rel(..., Supersedes).len() == 2` with both target document ids and paths present.

## 5. `unknown` default guarantee

| Layer | Mechanism |
| --- | --- |
| SQL | `status TEXT NOT NULL DEFAULT 'unknown'` |
| Rust type | `DocumentStatus` derives `Default` → `Unknown` |
| Register API | `status: Option<DocumentStatus>`; `None` → `DocumentStatus::Unknown` |
| Test | `unclassified_document_status_is_unknown_not_active` asserts `== Unknown` and `!= Active` on return value **and** reloaded row |

Migration test also asserts `pragma_table_info` default text contains `unknown` and not `active`.

## 6. Nine required tests — coverage map

| # | Requirement | Test |
| --- | --- | --- |
| 1 | Migration idempotent | `migration_0010_is_idempotent_and_creates_knowledge_tables` (double open + column/CHECK probes) |
| 2 | 1→2 supersedes (real handoff) | `one_to_two_supersedes_handoff_case` |
| 3 | Both link endpoints empty rejected | `link_rejects_both_endpoints_empty` (Rust + SQL CHECK) |
| 4 | Pathless supersession claim | `pathless_supersession_claim_registers_with_to_path_only` |
| 5 | Illegal role / status rejected | `illegal_role_and_status_rejected_by_parse` + migration SQL CHECK inserts |
| 6 | Unclassified → `unknown` not `active` | `unclassified_document_status_is_unknown_not_active` |
| 7 | Path SSoT (`COM¹` rejected) | `path_validation_uses_ssot_rejects_superscript_com` + unit twin |
| 8 | Project isolation | `documents_are_isolated_by_project` |
| 9 | Restart recovery | `restart_recovers_documents_and_links` |

Extra (not required but useful): `adding_supersedes_link_does_not_auto_change_status`.

## 7. Repository API surface

`SqliteKnowledgeRepository`:

| Method | Purpose |
| --- | --- |
| `open` | Open DB, `PRAGMA foreign_keys=ON`, apply migrations |
| `register` | Insert document (status default `unknown`) |
| `update` | Explicit field update (status only when `Some`) |
| `get` | By `document_id` |
| `get_by_path` | By `(project_id, path)` |
| `list_by_project` | Project-scoped list |
| `add_link` | Write one multi-valued edge |
| `list_links_from` | All outbound links |
| `list_links_from_with_rel` | Outbound filtered by `rel` |

## 8. Files touched (whitelist only)

| Path | Action |
| --- | --- |
| `crates/storage-sqlite/migrations/0010_knowledge.sql` | **new** |
| `crates/storage-sqlite/src/knowledge.rs` | **new** |
| `crates/storage-sqlite/src/migrations.rs` | wire `0010` |
| `crates/storage-sqlite/src/lib.rs` | `mod` + re-exports |
| `crates/storage-sqlite/tests/knowledge_repository.rs` | **new** |
| `docs/execution/2026-08-08-knowledge-registry-k0a-report.md` | **this report** |

**Cargo.toml:** not modified (no new deps).

## 9. Not touched (hard bans)

- `apps/desktop/**`
- `crates/kernel-project/**` (converged portable path — read-only use)
- `crates/git-executor/**`, `crates/kernel-resource/**`, `crates/runtime-host/**`
- `scripts/**`, `tests/fixtures/**`
- Any other crate `Cargo.toml`
- No `docs/` scan / bulk registration / retirement / archive logic
- No secret fields
- No git commit/add/stash/reset/clean/checkout
- No desktop E2E
- No auth/token reads

## 10. Commands and exit codes

| Command | Exit |
| --- | --- |
| `cargo test -p keeper-storage-sqlite --test knowledge_repository` | **0** (10 passed) |
| `cargo test -p keeper-storage-sqlite knowledge` | **0** (unit + filtered) |
| `cargo fmt -p keeper-storage-sqlite -- --check` | **0** |
| `node scripts/run-worker-gate.mjs` | **0** (`PILLOW_WORKER_GATE: OK`) |

Worker gate includes: workspace check, secrets check, `cargo fmt --all --check`, pnpm typecheck, pnpm test, cargo clippy (workspace), cargo test (workspace subset per gate), ACP protocol drift check.

## 11. Residual risks / uncertainties

1. **No delete/unlink API** in K0a — links are append-only from this slice; callers cannot retract a wrong edge without raw SQL. Acceptable for storage-first slice; K0b+ may need controlled unlink.
2. **`to_path` free-text vs path-shaped:** free-text claims (no `/` and no `.`) skip portable-path SSoT so “prior delivery status” works. Path-shaped `to_path` values still go through SSoT. Edge case: a claim that contains a period but is not a path is treated as path-shaped — reject if illegal under SSoT.
3. **Foreign keys require real `projects` rows** — registration against a missing project_id fails at SQLite (mapped as generic `Sqlite`). Callers must seed projects first (same pattern as settings/conversations).
4. **No content hashing helper** — repository stores the digest string; callers compute bytes→digest. Intentional for storage-only scope.
5. **Keeper bridge boot** returned HTTP 400 in this session environment; work proceeded on the written task contract (record-only Keeper policy).

## 12. Completion

**K0A-REGISTRY-COMPLETE**
