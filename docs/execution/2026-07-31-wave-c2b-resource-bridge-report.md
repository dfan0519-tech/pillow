# Pillow C2-B: Project Resource Bridge — Completion Report

**Date**: 2026-07-31
**Status**: `DONE`
**Worker**: grok3 (controller lease)
**Scope**: `this repository` only; C2-A / C1 / B3 files outside whitelist untouched.
**Commit**: not staged, not committed (per brief).

---

## Changed files (this task)

| Path | Role |
|------|------|
| `apps/desktop/src-tauri/Cargo.toml` | Depend on `keeper-resource` |
| `apps/desktop/src-tauri/src/app_state.rs` | Open recents repo; project-id resolve; recent record/list accessors |
| `apps/desktop/src-tauri/src/lib.rs` | Register resource commands; unit-test module |
| `apps/desktop/src-tauri/src/resource_commands.rs` | **New** camelCase DTOs + Tauri commands + AppState bridge ops |
| `apps/desktop/src-tauri/src/resource_commands_tests.rs` | **New** project isolation / fail-closed tests |
| `apps/desktop/src/lib/resourceGateway.ts` | **New** typed gateway contract + opaque error normalization |
| `apps/desktop/src/lib/tauriResourceGateway.ts` | **New** Tauri invoke boundary with exact payload validation |
| `apps/desktop/tests/resource-gateway.test.ts` | **New** TypeScript contract tests (TDD first) |
| `docs/execution/2026-07-31-wave-c2b-resource-bridge-report.md` | This report |

**Cargo-generated (not on strict whitelist, required for desktop dep graph):**
`Cargo.lock` — adds `keeper-resource` edge under `pillow-desktop` (and any missing C2-A package entries already present in the dirty tree).

**Not modified by this task (pre-existing dirty C1/B3):** conversation service, UI, provider/runtime, C2-A crates.

---

## TDD evidence

### RED (implementation absent)

| Command | Exit | Outcome |
|---------|------|---------|
| `pnpm --filter @pillow/desktop test -- resource-gateway` | 1 | `Failed to resolve import "../src/lib/resourceGateway"` (and `tauriResourceGateway`) — suite failed before any test body ran |
| `cargo test -p pillow-desktop resource` | 0* / empty | Filter matched **0** unit tests (test module not yet wired; production methods absent) |

\*Exit code noise from PowerShell writing cargo progress to stderr; result showed `running 0 tests` for lib unit tests.

Focused test files were written **before** production modules:

- `apps/desktop/tests/resource-gateway.test.ts`
- `apps/desktop/src-tauri/src/resource_commands_tests.rs`

### GREEN (after implementation)

| Command | Exit | Outcome |
|---------|------|---------|
| `cargo test -p pillow-desktop resource` | 0 | **6** resource unit tests passed |
| `pnpm --filter @pillow/desktop exec vitest run tests/resource-gateway.test.ts` | 0 | **21** tests passed |
| `pnpm --filter @pillow/desktop typecheck` | 0 | Clean |
| `cargo fmt --all -- --check` | 0 | Clean after `cargo fmt --all` on whitelist files |
| `git diff --check` | 0 | No whitespace errors (CRLF warnings only) |

### Final verification (re-run at completion)

```
cargo test -p pillow-desktop resource
  → 6 passed (resource_commands_tests::*)

pnpm --filter @pillow/desktop exec vitest run tests/resource-gateway.test.ts
  → 21 passed

pnpm --filter @pillow/desktop typecheck
  → pass

cargo fmt --all -- --check
  → pass

git diff --check
  → pass (LF/CRLF warnings only)
```

### Behavior covered by tests

**TypeScript boundary**

- Every operation requires `projectId` and uses snake_case Tauri command names
- Optional `relativePath` forwarded for list/search
- camelCase DTO acceptance for list / file / search / recent
- Binary preview metadata accepted; smuggled `bytes` rejected
- Missing fields, bad enums, non-arrays, absolute/machine paths rejected
- Extra `projectRoot` / `nativeHandle` keys rejected
- Allow-listed opaque host errors preserved; path-bearing / unknown errors normalized

**Rust isolation**

- Unknown / empty / malformed project id fails closed on all four ops (no cwd / last-project fallback)
- Two projects: list/read isolated; cross-path miss is `resource_not_found` without content/path leakage
- Invalid relative paths rejected without machine-path error text
- Successful read updates only the matching project's recents
- Binary read returns `contentKind: "binary"` with no content and no serialized `bytes` field

---

## Public contract delivered

Renderer API (`ResourceGateway`):

- `listProjectResources({ projectId, relativePath? }) -> ResourceEntry[]`
- `readProjectResource({ projectId, relativePath }) -> ResourceFile`
- `searchProjectResources({ projectId, query, relativePath? }) -> ResourceSearchResult[]`
- `listRecentProjectResources({ projectId }) -> RecentResource[]`

Tauri commands (camelCase args/results):

- `list_project_resources`
- `read_project_resource`
- `search_project_resources`
- `list_recent_project_resources`

Resolution rules:

1. `projectId` required; root taken only from `SqliteProjectRepository::get`
2. Stale/unknown id → `project_not_found` (never cwd / most-recent project)
3. Operations delegated to `keeper-resource::ResourceReader` + `SqliteResourceRecentRepository`
4. Successful reads call `record` only for that project id + portable path
5. Binary/oversized: bridge strips raw `bytes`; UTF-8 content only when core reports utf8
6. No mutation commands; no renderer filesystem API

---

## Residual risks / known limitations

1. **TypeScript portable-path check is slightly stricter than C2-A** for components containing the substring `..` (e.g. `foo..bar`). Fail-closed at the renderer boundary; core may still accept such a leaf name if ever requested via a different client.
2. **Search case sensitivity** is fixed to case-insensitive in the bridge (`case_sensitive: false`). The public TS contract does not expose a case flag; narrow interpretation.
3. **List/search truncation flag** from C2-A `ResourceDirectory` / `ResourceSearchResults` is not surfaced — the public contract returns bare arrays. Clients cannot distinguish “empty” from “truncated at cap” without a later field.
4. **Recents open error mapping** collapses SQLite open failures to `ProjectStoreError::LockPoisoned` to avoid a direct `rusqlite` dependency in desktop. Production open still fails closed; diagnostics are coarser.
5. **Cargo.lock** updated for the desktop → `keeper-resource` edge; multi-wave dirty tree remains unstaged.
6. **No UI wiring** — ResourcePane still shows conversation references only; this slice is bridge-only.

---

## Staging / commit

- **Not staged**
- **Not committed**
- Whitelist-only intentional edits; pre-existing C1/B3 dirt left as found

---

## Keeper completion message

```
Pillow C2-B project resource bridge complete (unstaged).
Typed Tauri + TS gateway: list/read/search/recent, projectId-required,
fail-closed unknown project, portable paths only, binary bytes stripped.
Tests: cargo test -p pillow-desktop resource (6 ok);
vitest resource-gateway (21 ok); typecheck; fmt check; git diff --check.
No mutation, no renderer FS, no C2-A/UI/C1 edits beyond whitelist.
Next: C2-C resource UI (if planned).
```
