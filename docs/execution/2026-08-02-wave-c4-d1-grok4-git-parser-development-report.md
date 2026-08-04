# C4-D1 Grok4 development report — read-only Git parser foundation

Date: 2026-08-02

Worker: real Grok CLI slot `grok4`
Worker session: `<keeper-session>`
Controller parent session: `<keeper-session>`
Task: `c4-d1-git-parser`
Status: **DONE_WITH_CONCERNS** (not controller-accepted)

## Objective

Create pure parser/bounds crate `keeper-git-executor` for C4 Task 3: porcelain
v2 `-z` status parsing and one-file diff bounds. No child process launch and no
external crate dependencies.

## Public boundary delivered

| Symbol | Role |
| --- | --- |
| `GitRepositoryId` | Project-relative repository identity |
| `GitHead` | `Branch { name, oid }` / `Detached { oid }` / `Unknown` |
| `GitStatusKind` | modified/added/deleted/untracked/renamed/copied/typechange/conflict/submodule/ignored/unknown |
| `GitStatusEntry` | `path`, `old_path: Option`, `kind`, `xy: [u8; 2]` |
| `GitStatusSnapshot` | repository, head, entries, truncated, generation |
| `GitDiffView` | `Text` / `Binary` / `Oversized` typed summaries |
| `parse_porcelain_v2_z` | NUL-record porcelain v2 parser |
| `bound_git_diff` | one-path diff classifier + bounds |
| `STATUS_ENTRY_CAP` | **5_000** |
| `STATUS_TRANSPORT_CAP` | **2 MiB** |
| `DIFF_TEXT_BYTE_CAP` | **256 KiB** |
| `ParseError` | `Malformed` / `PathRejected` / `Utf8` |

## Parser coverage

| Record / state | Fixture coverage |
| --- | --- |
| Ordinary tracked (`1 …`) | modified entry; XY `.M` |
| Untracked (`?`) | path + kind `Untracked` |
| Rename (`2` + score + path NUL + oldPath NUL) | one entry, `old_path` set, kind `Renamed` |
| Conflict unmerged (`u`) | kind `Conflict`, XY `UU` |
| Submodule ordinary (`sub` starts with `S`) | kind `Submodule` |
| Detached HEAD header | `# branch.head (detached)` + oid |
| Mixed multi-state stream | all five kinds in one snapshot |
| Ignored (`!`) | parse path present (shape covered in API) |

Portable path grammar: relative, `/` only, no `..`, no drive/UNC, no Windows
device names, no control/illegal characters (aligned with resource path rules).

## Malformed / bounds cases

| Case | Result |
| --- | --- |
| Trailing partial record (no final NUL) | `ParseError::Malformed` |
| Ordinary wrong field count | `ParseError::Malformed` |
| Rename missing original-path record | `ParseError::Malformed` |
| Absolute / `..` / backslash paths | `ParseError::PathRejected` |
| Entry count > 5_000 | keep first 5_000, `truncated: true` |
| Transport > 2 MiB | parse complete records within window, `truncated: true` |
| Diff contains NUL | `GitDiffView::Binary` (no text body) |
| Diff length > 256 KiB | `GitDiffView::Oversized` (no partial text) |
| Diff at exact 256 KiB text | `GitDiffView::Text` accepted |

## Git version assumption

Fixtures mirror machine output of **Git for Windows 2.54.x**
`git status --porcelain=v2 -z --branch`:

- Headers and records are NUL-terminated.
- Rename score field is space-separated (`R100`), then path, then a second
  NUL-terminated original path field.
- Ordinary submodule uses 4-char `sub` field starting with `S`.

No Git binary is invoked by this crate.

## TDD gates

### RED (expected compile failure before implementation)

Package did not exist; first `cargo test -p keeper-git-executor` would fail to
resolve the package. Implementation landed with fixtures in the same worker
turn.

### GREEN (scoped package)

```text
$env:RUSTUP_HOME = "%USERPROFILE%\.rustup"
$env:CARGO_HOME = "%USERPROFILE%\.cargo"
cargo test -p keeper-git-executor --tests
# 19 passed (13 status_porcelain + 6 bounded_diff)

cargo check -p keeper-git-executor --all-targets
# exit 0

cargo fmt -p keeper-git-executor --check
# exit 0

git diff --check -- Cargo.toml crates/git-executor
# exit 0
```

### Workspace check (concern)

```text
cargo check --workspace --all-targets
# FAIL outside allowlist:
# crates/tool-executor/src/lib.rs: non-exhaustive match on
# ToolArguments::ProcessRun(_) — pre-existing concurrent C4-D1 protocol work.
```

`cargo fmt --all --check` also reports pre-existing formatting drift in
`crates/agent-protocol/**` (not in this allowlist). Not fixed here.

### Cargo.lock

Brief forbids modifying `Cargo.lock`. Cargo briefly rewrote it when resolving
the new workspace member (and pulled broader stale package metadata). **Lock was
restored to HEAD and left unstaged.** Controller should regenerate
`Cargo.lock` once when merging the full C4-D1 package set.

Note: `node scripts/run-cargo.mjs` failed in this environment because
`RUSTUP_HOME` pointed at `%USERPROFILE%\.grok4\.rustup` with no active
toolchain. Direct `cargo` with user rustup home worked.

## Changed files (allowlist)

| Path | Action |
| --- | --- |
| `Cargo.toml` | **Modified** — workspace member `crates/git-executor` |
| `crates/git-executor/Cargo.toml` | **Created** — zero dependencies |
| `crates/git-executor/src/lib.rs` | **Created** — re-exports |
| `crates/git-executor/src/status.rs` | **Created** — porcelain parser + bounds |
| `crates/git-executor/src/diff.rs` | **Created** — `bound_git_diff` |
| `crates/git-executor/tests/status_porcelain.rs` | **Created** — 13 fixtures |
| `crates/git-executor/tests/bounded_diff.rs` | **Created** — 6 bounds tests |
| `docs/execution/2026-08-02-wave-c4-d1-grok4-git-parser-development-report.md` | **Created** — this report |

**Not modified (deliberate):** `Cargo.lock` (restored), any process/runner code,
staging, commit.

## Process / dependency guarantees

- No `std::process`, no `Command`, no sockets, no filesystem Git discovery.
- `[dependencies]` empty in crate manifest.
- No new workspace.dependencies entries.

## Risks / follow-ups

1. **Workspace check red** until sibling Task 1/2/5 wire `ProcessRun` exhaustively
   in `tool-executor` (outside this allowlist).
2. **Cargo.lock** not updated; first clean workspace `cargo generate-lockfile` /
   normal build by controller will need an intentional lock commit.
3. Parser does not yet resolve nested-repo ownership or invoke Git; Task 9
   runner must supply trusted porcelain bytes and repository identity.
4. Kind classification is XY/sub-field heuristic; copy (`C`) and typechange
   (`T`) are supported in the enum and XY map but not given dedicated fixtures
   beyond rename/ordinary paths.
5. Concurrent dirty tree from other C4-D1 workers; this worker only edited the
   allowlist paths above.

## Staging / commit

Empty intentional staging from this worker. **No commit.**

## Keeper signal

`C4-D1-Grok4-GitParser-DONE_WITH_CONCERNS — keeper-git-executor pure porcelain v2 -z parser + 256KiB diff bounds; 19/19 package tests green; workspace check blocked by pre-existing ProcessRun exhaustiveness outside allowlist; Cargo.lock restored unstaged.`
