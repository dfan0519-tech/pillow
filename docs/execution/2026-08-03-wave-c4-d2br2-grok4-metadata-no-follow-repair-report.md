# C4 D2BR2 Grok4 — Git metadata no-follow repair report

Date: 2026-08-03
Worker: real Grok CLI `.grok4` (model Grok 4.5)
Worker session: `<keeper-session>`
Controller parent session: `<keeper-session>`
Task: `c4-d2br2-git-metadata-no-follow`
Status: **COMPLETE** (not controller-accepted)

## Why D2BR1 was rejected

D2BR1 fixed worktree diff containment and fail-closed config-wire parsing, but
controller source review found that repository **metadata** reads still followed
file/directory links before rejecting:

| Site | Prior behaviour |
| --- | --- |
| `scan_git_config_files` | `is_file` / `read_to_string` followed symlinks |
| `common_dir_from_gitdir` | `is_file` / `read_to_string` / `canonicalize` followed reparse `commondir` |
| `deny_active_hooks` | `is_dir` / `read_dir` / `is_file` followed reparse hooks dir and entries |
| `scan_gitattributes` | `is_file` / `read_to_string` followed worktree/gitdir/common attributes |

Any of these could read or enumerate outside the active project authority.

## Exact changed files (allowlist only)

| Path | Action |
| --- | --- |
| `crates/git-executor/src/runner.rs` | **Modified** — no-follow metadata sites, early FS scan, `--no-includes`, whitespace include classification |
| `crates/git-executor/src/discovery.rs` | **Modified** — contained open/read helpers, component-wise resolve, bounded UTF-8 metadata reads |
| `crates/git-executor/tests/git_read_integration.rs` | **Modified** — RED→GREEN no-follow + include + oversize/utf8 fixtures |
| `docs/execution/2026-08-03-wave-c4-d2br2-grok4-metadata-no-follow-repair-report.md` | **Created** — this report |

**Not modified:** manifests/lock, protocol, policy, process-executor, storage,
tool-executor, runtime-host, Tauri, renderer, scripts. No stage / commit /
reset / restore / clean / package / publish.

## Repair summary

### 1. Shared no-follow metadata primitives (`discovery.rs`)

- `meta_is_symlink_or_reparse` — symlink **or** Windows `FILE_ATTRIBUTE_REPARSE_POINT`
- `open_contained_entry` — `symlink_metadata`, reject reparse, require file/dir, canonicalize, prove under project
- `resolve_under_base_no_follow` — component-wise walk (including `..`), reject reparse **ancestors and leaf**
- `read_contained_metadata_file` / `read_metadata_under_base` — bounded reads (`METADATA_FILE_BYTE_CAP` = 256 KiB); oversized → `MalformedConfiguration`; invalid UTF-8 → `Parse(Utf8)`
- `parse_gitfile` — bounded no-follow read + contained gitdir resolve; outside → `LinkedWorktreeOutsideProject`

### 2. Every metadata read site (`runner.rs`)

| Site | No-follow behaviour |
| --- | --- |
| `.git` marker | reparse rejected before resolve |
| `commondir` file | `read_metadata_under_base`; target path component-wise contained |
| hooks directory | `resolve_under_base_no_follow(..., "hooks", dir)`; reparse dir → `PathRejected` |
| hook entries | `symlink_metadata` per entry; reparse/symlink → `UnsafeConfiguration::Hook` |
| `config` / `config.worktree` | under `common_dir` and `gitdir` via `read_metadata_under_base` |
| worktree `.gitattributes` | `read_metadata_under_base(workdir, ".gitattributes")` |
| gitdir/common `info/attributes` | `read_metadata_under_base(..., "info/attributes")`; missing mid-path → absent (not reject) |

### 3. Filesystem scan before any Git process

`deny_filesystem_metadata` (hooks + raw config + attributes) runs in:

1. `prepare_repo` — **before** `rev-parse` / `show-toplevel`
2. `classify_worktree` — **before** `show-toplevel`
3. `deny_unsafe_configuration` — again before the git config probe

This ensures whitespace-separated `include` lines and linked config/attributes
are denied **before** Git would load or follow them.

### 4. Config include classification

Raw INI parser (`classify_config_ini` / `parse_config_assignment_line`):

- `key = value` and `key=value`
- **whitespace-separated `key value` without `=`** (e.g. `[include]` / `path ../out.inc`)
- bare `key` → boolean true
- malformed section / empty key → `MalformedConfiguration` (fail closed)
- `include.path` / `includeIf.*.path` → `UnsafeConfigKind::Include`

### 5. Git config probe no-includes

`GitTemplate::ConfigLocalList` argv is fixed:

```text
config --local --list --null --no-includes
```

Git is not permitted to expand `include` / `includeIf` during the local probe.
Raw contained config still independently classifies include directives.

### 6. Preserved D2BR1 surface

Config-wire `key\nvalue\0`, output reliability, untracked/symlink/junction diff
containment, outside-gitdir unavailability, internal linked-worktree
`info/attributes`, generation-bearing `GitDiffSnapshot`, fixed read-only
templates, clean env, no public argv API.

## RED → GREEN evidence

### New unit tests (`runner::tests`, 13 total)

| Case | Result |
| --- | --- |
| whitespace include without `=` → Include | GREEN |
| equals include still denied | GREEN |
| malformed section / empty key fail closed | GREEN |
| prior config-null-wire + reliability suite | GREEN |

### New integration tests (28 total integration)

| Case | Result |
| --- | --- |
| `config_symlink_outside_project_is_rejected` | GREEN (`PathRejected`) |
| `config_worktree_junction_outside_is_rejected` (Windows file link) | GREEN |
| `commondir_file_symlink_outside_is_rejected` | GREEN |
| `hooks_directory_junction_outside_is_rejected` (Windows) | GREEN |
| `hooks_entry_symlink_outside_is_denied` | GREEN |
| `worktree_gitattributes_symlink_outside_is_rejected` | GREEN |
| `gitdir_info_attributes_symlink_outside_is_rejected` | GREEN |
| `whitespace_include_without_equals_denied_before_outside_read` | GREEN (Include before outside read) |
| `oversized_and_invalid_utf8_metadata_fail_closed` | GREEN |
| prior real config-wire, junction discovery, untracked diff, linked worktree, status/diff, generation | GREEN |

### Gate commands

```text
cargo test -p keeper-git-executor --test git_read_integration -- --nocapture
# 28 passed; 0 failed

cargo test -p keeper-git-executor --tests
# unit 13 + bounded_diff 6 + git_read_integration 28 + status_porcelain 15
# = 62 passed; 0 failed

cargo check --workspace --all-targets
# exit 0

cargo clippy -p keeper-git-executor --all-targets -- -D warnings
# exit 0

cargo fmt --all --check
# exit 0

git diff --check -- crates/git-executor
# exit 0
```

## Config include proof

1. **Raw whitespace form** (no `=`): fixture plants
   `[include]\n\tpath <absolute-outside.inc>\n` into `.git/config` with a canary
   body in the outside file. Status returns
   `UnsafeConfiguration { Include }` from the filesystem scan **before** any
   `git` process runs (`prepare_repo` order).
2. **Unit**: `config_ini_whitespace_include_without_equals_is_denied` classifies
   both `[include] path ../x` and `[includeIf "…"] path /tmp/x`.
3. **Git probe**: template carries `--no-includes` so `git config --local --list
   --null` does not expand includes even if raw scan were bypassed.
4. **Equals form** still denied (prior D2BR1 real-wire fixture remains green).

## Staging / HEAD state

- HEAD: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b` (unchanged)
- Staging: empty for this allowlist (no `git add` / commit)
- Allowlist paths remain untracked under `crates/git-executor/` and the new report
- Pre-existing dirty C1–C4 tree outside allowlist is untouched

## Residual processes and risks

- Residual processes: none spawned by this worker beyond short-lived
  `cargo test` / fixture `git` / `mklink` children (all exited).
- Risks / residual:
  1. File-symlink fixtures require Windows symlink privilege; tests soft-skip
     only when `mklink` fails (junction tests still hard-require `/J`).
  2. `prepare_repo` still launches `rev-parse` after the FS metadata scan; if
     an attacker races a config swap between scan and git spawn (TOCTOU), the
     git child could observe different content. Fail-closed on subsequent
     probe remains; full open-by-fd race immunity is out of this slice.
  3. `show_toplevel` still canonicalizes Git’s printed path without a full
     reparse walk of every component of that string (Git-owned output under
     an already-contained workdir).
  4. Controller alone accepts this repair.

## COMPLETION

`COMPLETION: complete`
