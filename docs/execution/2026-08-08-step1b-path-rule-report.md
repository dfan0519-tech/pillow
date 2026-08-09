# Step 1b — Portable path rule single source of truth

**Date:** 2026-08-08  
**Signal:** `STEP1B-PATH-RULE-COMPLETE`  
**Worker:** grok-step1b (Keeper boot skipped: worker claims require local parent session id)

## Why

The same portable-path invariant had been reimplemented independently in the
porcelain parser, project-path mapping, Tauri command gate, and TypeScript DTO
layer. Collapsed porcelain directory entries (`? vendor/`) were accepted in one
layer and rejected in another, so status degraded to `git_unavailable` after
partial “fixes.”

## Where the rule lives (Rust SSoT)

**Module:** `crates/git-executor/src/portable_path.rs`

| Export | Role |
| --- | --- |
| `validate_portable_path` / `is_portable_path` | File form: repository ids + diff targets |
| `validate_portable_path_or_directory` / `is_portable_entry_path` | Status entry form: allows one trailing `/` |
| `PORTABLE_PATH_MAX_LEN` (4096) | Full path length cap |
| `PORTABLE_COMPONENT_MAX_LEN` (255) | Per-segment cap |

Public crate surface: `keeper_git_executor::{is_portable_path, is_portable_entry_path, PORTABLE_PATH_MAX_LEN, PORTABLE_COMPONENT_MAX_LEN}`.

TypeScript cannot share Rust code. The renderer mirror lives in
`apps/desktop/src/lib/tauriGitGateway.ts` (`portable` / `portableEntryPath`,
exported as `isPortablePath` / `isPortableEntryPath` for contract tests only).

## Call-site changes

| Layer | Before | After |
| --- | --- | --- |
| Parser | Local `validate_portable_path*` in `status.rs` | Thin map onto `portable_path` → `ParseError::PathRejected` |
| Discovery | Already called status helpers | Unchanged import path (`status` wrappers → SSoT) |
| Diff / runner | Called status helpers | Still via status wrappers / same SSoT |
| Command gate | Local `is_portable_path` in `git_commands.rs` (1024 cap, no device names) | `use keeper_git_executor::is_portable_path` |
| TS DTO | Independent `portable` / `portableEntryPath` (1024, no devices/trailing dot-space) | Full mirror of SSoT (4096, devices, trailing `.` / space) |

**Unchanged by design:** public error codes and user-facing messages
(`git_path_rejected`, `git_unavailable` DTO copy, etc.).

**Semantics preserved:**

- `vendor/` accepted **only** on status entry paths (entry form).
- Repository id and diff path remain file form (directory form rejected).
- Traversal, absolute paths, native separators, empty segments, control chars,
  Windows device names rejected on both forms.

## Cross-layer contract

**Shared table:** `tests/fixtures/portable-path-cases.json`

| Form | Accept | Reject | Subtotal |
| --- | ---: | ---: | ---: |
| `fileForm` | 3 | 20 | 23 |
| `entryForm` | 4 | 19 | 23 |
| **Total case rows** | | | **46** |

Required minimums covered: `a.txt`, `src/lib.rs`, `vendor/` (entry only), `.`
(repo id / file form), `/`, `//`, `vendor//`, `./`, `../escape`, `../escape/`,
`vendor/../escape/`, `C:/x`, `a\b`, `/absolute`, overlong component, control
character, `CON` / `con.txt` / `COM1` / `NUL` / nested device.

**Rust:** `crates/git-executor/tests/portable_path_contract.rs` loads the JSON
and asserts `is_portable_path` / `is_portable_entry_path`.

**TypeScript:** `apps/desktop/tests/tauri-git-gateway.test.ts` describe
`portable path cross-layer contract` loads the same JSON and asserts
`isPortablePath` / `isPortableEntryPath`.

Drift on either side fails that side’s suite.

## Gate results

| Command | Exit |
| --- | ---: |
| `node scripts/run-cargo.mjs test --workspace` | **0** (final run with `CARGO_TEST_THREADS=1`) |
| `node scripts/run-cargo.mjs clippy --workspace -- -D warnings` | **0** |
| `node scripts/run-cargo.mjs fmt --all -- --check` | **0** |
| `corepack pnpm --filter @pillow/desktop exec vitest run tests/tauri-git-gateway.test.ts --no-cache` | **0** (24 tests) |
| `corepack pnpm --filter @pillow/desktop typecheck` | **0** |
| `node scripts/check-secrets.mjs` | **0** |
| `git diff --check` | **0** |
| `git diff --cached --name-only` | empty |

Notes on intermediate failures (not caused by this change set):

- First workspace build: `LNK1104` on `governed_process_bridge` (Windows linker
  file lock under parallel compile).
- Parallel test runs occasionally failed unrelated flakes:
  - `keeper-runtime-host::governed_process_bridge::slow_output_is_visible_live_and_reopens_after_waiter_consumes_terminal`
  - `pillow-desktop::conversation_service::tests::composition::injected_adapter_routes_probe_session_turn_and_cancel`
  Both pass in isolation; full workspace green under single-thread test
  execution. Out of write whitelist for this task.

## Residual risks

1. **TS is a mirror, not generated code.** The shared JSON locks accept/reject
   pairs, but a future hand-edit of only one side without updating the fixture
   will fail CI only if that case is in the table.
2. **Length metric:** Rust uses `str::len` (UTF-8 bytes); TS uses JS string
   length (UTF-16 code units). ASCII-only product paths match; exotic Unicode
   could theoretically diverge. Not in the shared table.
3. **Other portable-path copies** remain outside this step’s scope:
   `agent-protocol` tool path grammar, `kernel-resource`, `tauriExecGateway`
   portable cwd. Framework adapters must still be directed at the git SSoT (or
   a future shared crate) rather than re-copying.
4. **Desktop E2E not run** (controller-owned).

## Not done

- No git add/commit/stash/reset/clean/checkout/restore.
- No desktop E2E.
- No edits under `runtime-host`, `adapter-grok-acp`, `tool-executor`,
  `apps/desktop/src/styles`, `apps/desktop/src/features`, `scripts`.
- Did not put the rule in `agent-protocol` (git-executor is the git stack’s
  lowest layer and already owned the complete grammar).
- Did not unify agent-protocol’s path validator with this SSoT (out of scope;
  slightly different error strings and COM/LPT superscript variants).

## Files touched (this task)

- `crates/git-executor/src/portable_path.rs` (new)
- `crates/git-executor/src/lib.rs`
- `crates/git-executor/src/status.rs`
- `crates/git-executor/Cargo.toml` (dev-deps: serde, serde_json)
- `crates/git-executor/tests/portable_path_contract.rs` (new)
- `apps/desktop/src-tauri/src/git_commands.rs`
- `apps/desktop/src/lib/tauriGitGateway.ts`
- `apps/desktop/tests/tauri-git-gateway.test.ts`
- `tests/fixtures/portable-path-cases.json` (new)
- `docs/execution/2026-08-08-step1b-path-rule-report.md` (this file)
