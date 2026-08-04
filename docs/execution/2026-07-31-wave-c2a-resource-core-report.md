# Pillow C2-A: Read-Only Resource Core — Completion Report

**Date**: 2026-07-31
**Status**: `DONE`
**Worker**: grok-c2a (controller lease)
**Scope**: `<workspace>\pillow` only; Keeper / `<workspace>\VaultSystem\keeper` untouched.
**Commit**: not staged, not committed (per brief).

---

## Changed files (this task)

| Path | Role |
|------|------|
| `Cargo.toml` | Workspace member `crates/kernel-resource` |
| `Cargo.lock` | Lockfile update for `keeper-resource` |
| `crates/kernel-resource/Cargo.toml` | Package `keeper-resource` |
| `crates/kernel-resource/src/lib.rs` | Read-only resource core implementation |
| `crates/kernel-resource/tests/read_only_resource_core.rs` | Focused RED→GREEN contract tests |
| `crates/storage-sqlite/src/lib.rs` | Export recent-resource repository |
| `crates/storage-sqlite/src/migrations.rs` | Apply migration 0004 |
| `crates/storage-sqlite/src/resource_recent.rs` | Durable project-scoped recents |
| `crates/storage-sqlite/migrations/0004_resource_recents.sql` | Schema |
| `crates/storage-sqlite/tests/resource_recent_repository.rs` | Focused RED→GREEN recents tests |
| `docs/execution/2026-07-31-wave-c2a-resource-core-report.md` | This report |

Not modified for C2-A (already dirty from prior waves / not required):
`crates/storage-sqlite/Cargo.toml` (no new dependency needed; `tempfile`/`rusqlite` already present).

---

## TDD evidence

### RED (implementation absent)

| Command | Exit | Outcome |
|---------|------|---------|
| `cargo test -p keeper-resource` | 101 | `package ID specification 'keeper-resource' did not match any packages` |
| `cargo test -p keeper-storage-sqlite --test resource_recent_repository` | 101 | `unresolved imports ResourceRecentError, SqliteResourceRecentRepository` |

Focused test files were written **before** production modules:

- `crates/kernel-resource/tests/read_only_resource_core.rs`
- `crates/storage-sqlite/tests/resource_recent_repository.rs`

### GREEN (after implementation)

| Command | Exit | Outcome |
|---------|------|---------|
| `cargo test -p keeper-resource` | 0 | 8 integration tests passed |
| `cargo test -p keeper-storage-sqlite --test resource_recent_repository` | 0 | 5 integration tests passed |
| `cargo fmt --all -- --check` | 0 | Clean after `cargo fmt --all` |
| `git diff --check` | 0 | No whitespace errors |

### Final verification (re-run at completion)

```
cargo test -p keeper-resource
  → 8 passed

cargo test -p keeper-storage-sqlite --test resource_recent_repository
  → 5 passed

cargo fmt --all -- --check
  → pass

git diff --check
  → pass
```

### Behavior covered by tests

- List sorted by filename; reparse/symlink surfaced as `symlink` and not traversed
- Absolute, `..`, backslash, empty, and non-portable path forms rejected
- Read: UTF-8 vs binary, exact `bytes_returned` / `truncated`, 1 MiB hard cap
- Search: deterministic path order, result cap + truncation, skip binary/overlong lines, skip symlink descendants
- Migration 0004 applied idempotently on reopen
- Recents: project isolation, upsert timestamp, newest-first + path tie-break, limit cap 50

---

## Public API delivered

Package: **`keeper-resource`** (`crates/kernel-resource`)

- `ProjectResourceCapability::new(project_id, root)`
- `ResourceReader::{list, read, search}`
- Serializable request/response types: `ResourceListRequest`, `ResourceReadRequest`,
  `ResourceSearchRequest`, `ResourceDirectory`, `ResourceFile`, `ResourceSearchResults`,
  `ResourceEntry`, `ResourceSearchMatch`, kinds, and `ResourceError`

Package: **`keeper-storage-sqlite`**

- Migration `0004_resource_recents.sql` (`resource_recents` PK `(project_id, path)`)
- `SqliteResourceRecentRepository::{open, record, list}`
- `ResourceRecent` / `ResourceRecentError` / `MAX_RECENT_LIST` (50)

Hard caps:

| Bound | Value |
|-------|-------|
| List entries | min(caller, 1_000) |
| Read bytes | min(caller, 1_048_576) |
| Search results | min(caller, 1_000) |
| Search candidate files | 100_000 |
| Search line / preview | 65_536 / 2_048 bytes |
| Recent list | min(caller, 50) |

---

## Residual risks

1. **Windows symlink privilege**: Creating directory junctions/symlinks without
   SeCreateSymbolicLinkPrivilege may skip the full reparse fixture path; the
   test still exercises listing and only asserts traversal when the reparse
   point was successfully created. Residual: rare environments without symlink
   rights get weaker automated reparse coverage.
2. **Search result hard cap**: Caller `max_results` is clamped to 1_000 (same
   order as list). The brief required a bound but did not name the constant;
   narrow interpretation used 1_000. C2-B may want an explicit protocol limit.
3. **No FK from `resource_recents.project_id` to `projects`**: Recents are
   string-keyed for later bridge flexibility; orphan rows are possible if
   projects are deleted without cleanup. Intentional for C2-A.
4. **Outside-root checks use `Path::starts_with` after `canonicalize`**: Matches
   B3 tool-executor style; unusual Windows long-path / `\\?\` edge cases remain
   theoretical residual risk.
5. **Working tree is multi-wave dirty**: Pre-existing uncommitted B3/C1 artifacts
   remain outside this whitelist; C2-A did not stage or commit them.

---

## Keeper completion message

```
Pillow C2-A read-only resource core complete (unstaged).
keeper-resource: list/read/search with portable path isolation, reparse refusal,
deterministic sort, 1 MiB / 1k list / search bounds.
keeper-storage-sqlite: migration 0004 resource_recents + project-scoped upsert list.
Tests: cargo test -p keeper-resource (8 ok);
cargo test -p keeper-storage-sqlite --test resource_recent_repository (5 ok);
fmt check + git diff --check pass.
No write/process/git/network/credential/Obsidian/MCP/runtime capability exposed.
Next: C2-B typed Tauri bridge.
```
