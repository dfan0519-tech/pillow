# C4 D2BR Grok4 — Git containment and fail-closed repair report

Date: 2026-08-03
Worker: real Grok CLI slot `grok4` (model Grok 4.5)
Worker session: `<keeper-session>`
Controller parent session: `<keeper-session>`
Task: `c4-d2br-git-containment-repair`
Status: **COMPLETE** (not controller-accepted)

## Why D2B was rejected (addressed)

Controller review showed:

1. Executable-configuration probe was ineffective against real Git output.
2. Multiple filesystem paths could cross the project authority boundary.

These were security acceptance blockers. This repair closes both classes.

## Exact changed files (allowlist only)

| Path | Action |
| --- | --- |
| `crates/git-executor/src/runner.rs` | **Modified** — fail-closed config wire parse, contained gitdir, attributes, generation-bearing diff, path probe |
| `crates/git-executor/src/discovery.rs` | **Modified** — Windows `FILE_ATTRIBUTE_REPARSE_POINT`, component-wise no-reparse walk/resolve |
| `crates/git-executor/src/diff.rs` | **Modified** — `GitDiffSnapshot { repository, generation, view }` |
| `crates/git-executor/src/lib.rs` | **Modified** — export `GitDiffSnapshot` |
| `crates/git-executor/tests/git_read_integration.rs` | **Modified** — containment + wire + generation RED→GREEN |
| `crates/git-executor/tests/bounded_diff.rs` | **Unchanged content intent** (still green; no API break for pure bounds) |
| `docs/execution/2026-08-03-wave-c4-d2br-grok4-git-containment-repair-report.md` | **Created** — this report |

**Not modified:** manifests/lock, protocol, policy, process-executor, storage,
tool-executor, runtime-host, Tauri, renderer, scripts. No stage / commit /
reset / restore / clean / package / publish.

## Repair summary

### 1. Fail-closed unsafe-configuration discovery

`deny_unsafe_configuration` no longer treats a failed `git config` probe as
safe. Spawn/process errors propagate; non-zero exit → `CommandFailed`;
truncated capture → `OutputUnreliable`; lossy UTF-8 → `Parse(Utf8)`;
malformed records → `MalformedConfiguration`.

New error variants:

- `GitReadError::MalformedConfiguration`
- `GitReadError::OutputUnreliable`
- `UnsafeConfigKind::Include` (local `include.path` / `includeIf.*.path`)

### 2. Real `git config --local --list --null` wire form

Live capture on this host proves records are:

```text
key\nvalue\0
```

Evidence (fixture `real_config_null_wire_detects_include_filter_textconv`):

- Host `git config --local --list --null` bytes contain
  `filter.evil.clean\nevil-clean-tool` (newline separator).
- `key=value\0` is **rejected** as malformed (unit test
  `config_null_wire_rejects_equals_form_as_malformed`), never silently ignored.
- Empty middle records / missing newline → `MalformedConfiguration`.

`parse_config_null_list` is the only accepted list parser.

### 3. Untracked path containment

Before any `git diff --no-index`, `probe_contained_file`:

- Walks every path component with `symlink_metadata`
- Rejects symlinks and Windows reparse points/junctions
- Canonicalizes each real component
- Proves the final regular file stays under workdir **and** project root

Symlink / junction untracked paths return `PathRejected` (never empty success
that could leak outside content).

### 4. Discovery reparse rejection

`walk_git_markers` and path helpers check
`FILE_ATTRIBUTE_REPARSE_POINT` (0x400) on Windows, not only
`file_type().is_symlink()`. Project-contained junctions to outside repos are
not traversed.

### 5. `.git` file / gitdir authority

`resolve_contained_git_dirs` resolves gitdir + common-dir (via `commondir` file
or conventional `worktrees/<name>` layout) and requires **both** remain under
the project root. Outside gitdir ⇒ `LinkedWorktreeOutsideProject` and discovery
`available: false`. No outside config/hooks/attributes are read.

Internal linked worktrees whose gitdir/common-dir stay inside the project are
accepted.

### 6. Diff generation contract

```rust
pub struct GitDiffSnapshot {
    pub repository: GitRepositoryId,
    pub generation: u64,
    pub view: GitDiffView,
}
```

`GitReadExecutor::diff(..., generation)` returns `GitDiffSnapshot` carrying the
repository identity and the **exact** input generation (no longer ignored).

### 7. Lossy / truncated status-diff fail-closed

- Status: any `truncated` or `lossy_utf8` → typed error; snapshot
  `truncated: true` from parser also maps to `OutputUnreliable`.
- Diff: lossy UTF-8 → `Parse(Utf8)`; truncation under text budget →
  `OutputUnreliable`; truncation/over size → typed `GitDiffView::Oversized`
  (no partial text publish).

### 8. Linked-worktree `info/attributes`

`scan_gitattributes` reads:

1. `workdir/.gitattributes`
2. `gitdir/info/attributes`
3. `common_dir/info/attributes`

Never assumes `workdir/.git` is a directory. Internal linked-worktree fixture
plants `* filter=evil` in main `.git/info/attributes` and proves denial on the
linked worktree path.

### 9. Preserved D2B safety surface

Fixed templates (`--no-pager`, `--no-optional-locks`, `-c core.fsmonitor=`,
`-c core.pager=cat`, `-c credential.helper=`), clean env, prompt/pager
suppression, `--no-ext-diff` / `--no-textconv`, output bounds, no-mutation
boundary, no public argv API.

## RED → GREEN evidence

### Unit tests (`runner::tests`, 9)

| Case | Result |
| --- | --- |
| `key\nvalue\0` unsafe detection (filter/include/textconv/fsmonitor) | GREEN |
| Safe list `Ok(None)` | GREEN |
| `key=value\0` rejected malformed | GREEN |
| Missing newline / empty middle record fail closed | GREEN |
| Empty values treated as disabled (safe) | GREEN |
| Truncation / lossy helpers fail closed; oversized truncation maps | GREEN |

### Integration tests (19 total; new containment cases)

| Case | Result |
| --- | --- |
| `real_config_null_wire_detects_include_filter_textconv` | GREEN |
| `untracked_diff_through_symlink_cannot_read_outside` | GREEN |
| `untracked_diff_through_junction_cannot_read_outside` (Windows) | GREEN |
| `discovery_cannot_traverse_project_junction_to_outside_repo` (Windows) | GREEN |
| `gitfile_resolving_outside_project_is_unavailable` | GREEN |
| `internal_linked_worktree_info_attributes_are_inspected` | GREEN |
| Diff snapshot generation + repository identity | GREEN |
| Ordinary / rename / binary / conflict / detached / oversized / fixed templates / no mutation | GREEN |

### Gate commands

```text
cargo test -p keeper-git-executor --test git_read_integration -- --nocapture
# 19 passed; 0 failed

cargo test -p keeper-git-executor --tests
# unit 9 + bounded_diff 6 + git_read_integration 19 + status_porcelain 15
# = 49 passed; 0 failed

cargo check --workspace --all-targets
# exit 0

cargo clippy -p keeper-git-executor --all-targets -- -D warnings
# exit 0

cargo fmt --all --check
# exit 0

git diff --check -- crates/git-executor
# exit 0
```

## Containment proof (live fixtures)

1. **Symlink untracked:** outside secret linked as `escape.txt` inside repo →
   `diff` returns `PathRejected`; no secret body in any success path.
2. **Junction untracked:** `mklink /J escapedir → outside` →
   `diff(..., "escapedir/secret.txt")` → `PathRejected`.
3. **Discovery junction:** `mklink /J portal → outside_repo` → discovery only
   surfaces project-root repo; no `portal` / outside identity.
4. **Outside gitdir:** main repo outside project, linked worktree inside →
   discovery `available: false`; status → `LinkedWorktreeOutsideProject`.
5. **Internal linked attributes:** main+wt inside project; common
   `info/attributes` filter → linked status `UnsafeConfiguration { Filter }`.

## Generation contract

- Status continues to echo input `generation` on `GitStatusSnapshot`.
- Diff now returns `GitDiffSnapshot { repository, generation, view }` with
  exact input generation (verified 777 vs 778 non-merge).

## Staging / HEAD / residuals

| Item | State |
| --- | --- |
| HEAD | `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b` preserved |
| Staging | empty for this worker’s allowlist (untracked `crates/git-executor/` + this report) |
| Residual processes | no owned cargo/rustc residuals retained after gates; no numeric-PID kill |
| Mutation | no product mutation verbs; fixture setup only uses host `git` in temps |

## Residual risks (controller)

1. **File symlink privilege:** if host cannot create file symlinks, the symlink
   integration test soft-returns early; junction tests are hard-required on
   Windows (mklink /J does not need admin).
2. **Process-executor lossy capture:** reliability depends on
   `BoundedOutput::{truncated,lossy_utf8}` flags; raw bytes are not re-exposed.
3. **Parser-level status truncation flags** in pure `parse_porcelain_v2_z`
   still set `truncated: true` for unit fixtures; the **runner** refuses to
   publish those as successful status (fail-closed). Changing the pure parser
   was outside allowlist necessity.
4. **Not controller-accepted.** Controller owns source review and acceptance.

## Keeper signal

`COMPLETION: complete` — C4 D2BR git containment + fail-closed repair: real
`key\nvalue\0` config wire, reparse/junction/symlink containment, outside
gitdir unavailable, generation-bearing `GitDiffSnapshot`, 49/49 package tests
green; clippy -D warnings 0; fmt 0; scoped diff-check 0; workspace check 0;
HEAD 7b7f70f; staging empty; no commit.
