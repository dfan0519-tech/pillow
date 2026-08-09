# C4 D3B4 Round 1A — Grok1 native Tauri Git report

**Worker:** `grok1` (Grok CLI default profile)  
**Date:** 2026-08-05  
**Task:** `c4-d3b4-native-git`  
**HEAD:** `62149be9eddd2288eb45fd9b8278641cf64c7a5f`  
**Staging:** empty; no commit created.

## Keeper boot

- Attempted `agentRole=worker` → HTTP 400 `worker claims require local parent session id`
  (dispatcher supplied no parent session).
- Record-only boot proceeded as `agentRole=controller` for the same project/task
  so a session was recorded without inventing a parent id.
- Session: `<session-id>`.
- No Vault ingest, prompt generation, or completion judgment requested.

## Changed files (allowlist only)

| Path | Action |
| --- | --- |
| `apps/desktop/src-tauri/src/git_commands.rs` | **Create** |
| `apps/desktop/src-tauri/src/git_commands_tests.rs` | **Create** |
| `apps/desktop/src-tauri/src/lib.rs` | Modify (module, handler, manage) |
| `apps/desktop/src-tauri/Cargo.toml` | Modify (`keeper-git-executor` dep) |
| `docs/execution/2026-08-05-wave-c4-d3b4-grok1-native-git-report.md` | **Create** |

No other files were written or reformatted by this worker.

## Architecture

- **`GitCommandState`** is managed next to `AppState` at desktop startup.
  - Host-only Git executable resolution (well-known paths + `PATH`, optional
    `PILLOW_HOST_GIT` env for controlled hosts). Path is canonicalized and
    fail-closed; missing/non-file → no executor → public `git_unavailable`.
  - Per-project authority map: latest JS-safe generation + portable repository
    id → availability from the last discovery.
- **Commands** (exact invoke names, camelCase args from renderer):
  - `discover_git_repositories(projectId)`
  - `get_git_status(projectId, repositoryId, generation)`
  - `open_git_diff(projectId, repositoryId, generation, path)`
- **Authority rules**
  - `projectId` resolves only via `AppState::get_registered_project` (native
    root comes from the registered project record, never from the renderer).
  - Discovery atomically replaces that project's allowed repositories and
    advances generation (`1…MAX_SAFE_INTEGER`, wraps to `1`).
  - Status/diff require exact current project generation; generation lower than
    latest → `git_generation_stale`; higher → `git_generation_mismatch`.
  - Repository must be present and `available: true` in that snapshot;
    missing/unavailable → `git_repository_not_found`.
  - Composition is only `GitReadExecutor::{discover,status,diff}` fixed
    templates. No argv/cwd/env/executable/root from the renderer.
- **DTO wire shape** matches `gitGateway.ts` / `tauriGitGateway.ts` field sets
  (camelCase): discovery, status (including optional `originalPath`), and diff
  (`text` with `unified`/`lineCount`/`byteCount`; `binary`/`unsupported`
  without partial text). No native paths, argv, executable, or debug fields.

## Public error mapping

| Condition | Code | Message (path-free) |
| --- | --- | --- |
| Unregistered / invalid project id | `git_project_not_found` | Git project was not found. |
| Non-portable / traversal path | `git_path_rejected` | Git path was rejected. |
| Repo missing from discovery or unavailable | `git_repository_not_found` | Git repository was not found. |
| Host Git missing; executor/process/config/output failures | `git_unavailable` | Git read capability is unavailable. |
| Client generation &lt; latest / no discovery yet | `git_generation_stale` | Git result is stale; refresh changes. |
| Client generation &gt; latest | `git_generation_mismatch` | Git repository identity changed; refresh changes. |

`GitReadError` variants map into the six public codes only; messages never
include filesystem roots or separators.

## Tests and exit codes

```text
node scripts/run-cargo.mjs test -p pillow-desktop git_commands -- --nocapture
→ exit 0; 14 passed; 0 failed (filter reached lib unit tests)

node scripts/run-cargo.mjs clippy -p pillow-desktop --all-targets -- -D warnings
→ exit 0

node scripts/run-cargo.mjs fmt --all -- --check
→ exit 0

git diff --check -- <allowlist>
→ exit 0
```

Focused coverage:

1. Missing project → `git_project_not_found`
2. Registered project discovers root repo
3. Host Git unavailable → `git_unavailable`
4. Discovery generation advances atomically
5. Stale generation → `git_generation_stale`
6. Future generation → `git_generation_mismatch`
7. Status before discovery → stale
8. Missing repository id → `git_repository_not_found`
9. Portable path rejection (native/traversal/backslash/empty)
10. Nested repositories publish portable ids; nested status works
11. Status/diff exact gateway field serialization (no authority leakage)
12. Path-free error serialization + public code enum
13. Invoke handler registers exact three commands (no mutation surface)
14. `GitCommandState` managed wiring present in `lib.rs`

Fixtures use temporary directories and the real host Git for setup only;
product commands never read or mutate the Pillow working tree.

## Residual risk

- No renderer bridge, workbench mounting, or desktop E2E in this slice
  (Rounds 1B / 2 / 3 own those).
- Unavailable-repo discovery (outside linked worktree / unsafe config) is
  preserved by the executor; command-layer tests assert missing/unavailable
  rejection and nested available ids, not a full unsafe-config fixture at the
  Tauri boundary (covered in `keeper-git-executor` integration tests).
- Host Git resolution uses well-known Windows paths + `PATH`; exotic installs
  may need `PILLOW_HOST_GIT` in controlled environments.
- `lib.rs` already carried unrelated dirty changes from prior waves; this
  worker only added the Git module registration and manage call.

## Tree hygiene

- No `git add`, `commit`, `reset`, `clean`, `restore`, or reformat of unrelated
  files.
- Pre-existing dirty tree preserved (C4 D3B1–D3B3 process work, docs, etc.).
- Allowlist artifacts remain unstaged.
- Full desktop E2E not run (out of scope for this brief).

## Completion

Native read-only Git authority is composed into the Pillow Tauri host with
generation-scoped discovery, status, and diff commands and focused gates green.

`C4-D3B4-GROK1-NATIVE-GIT-COMPLETE`
