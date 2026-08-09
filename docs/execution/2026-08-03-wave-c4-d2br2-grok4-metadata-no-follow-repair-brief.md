# Pillow C4 D2BR2 - Git metadata no-follow repair brief

## Worker and boundary

- Worker: actual Grok CLI `.grok4`, Grok 4.5 high.
- Keeper: boot as worker for `c4-d2br2-git-metadata-no-follow`, parent
  `<keeper-session>`.
- Work only in `this repository`; Keeper/references are read-only.
- Preserve HEAD `7b7f70f` and all dirty accepted/in-flight slices. No stage,
  commit, reset, restore, clean, package or publish.

## Controller rejection

D2BR1 fixed worktree diff containment, but controller source review found that
repository metadata reads still follow file/directory links before rejecting:

- `scan_git_config_files` calls `is_file`/`read_to_string` on config paths;
- `common_dir_from_gitdir` follows a linked/reparse `commondir` file;
- `deny_active_hooks` follows a linked/reparse hooks directory and entries;
- `scan_gitattributes` follows worktree/gitdir/common attributes files.

Any of these can read or enumerate outside the active project authority.

## Required repair

- Before every repository metadata read or directory enumeration, use
  `symlink_metadata`, reject symlink and Windows reparse points, require the
  expected regular-file/directory type, canonicalize, and prove the resolved
  path remains under the project root.
- Apply this to `.git/config`, `config.worktree`, `commondir`, hooks directory
  and entries, worktree `.gitattributes`, and gitdir/common
  `info/attributes`. Reject reparse ancestors as well as the final component.
- Bound metadata file reads; oversized or invalid UTF-8 metadata fails closed.
- Prevent Git itself from following includes during the local config probe,
  using a fixed no-includes template where supported. Raw contained config must
  still classify `include`/`includeIf`, including valid whitespace-separated
  syntax without `=`; malformed/ambiguous configuration fails closed.
- Preserve D2BR1 config-wire, output reliability, diff containment,
  outside-gitdir and generation fixes.

## RED -> GREEN evidence

Add failing-then-passing Windows-capable tests for:

- config/config.worktree symlink or junction target outside the project;
- linked/reparse `commondir` file or ancestor;
- linked/reparse hooks directory and hook entry;
- worktree and gitdir/common attributes links outside the project;
- valid include syntax with whitespace separator and no `=` is denied before
  an outside include is read;
- oversized and invalid-UTF-8 metadata fails closed;
- existing real config-wire, junction discovery, untracked diff, linked
  worktree, status/diff and generation suites remain green.

Run:

- `cargo test -p keeper-git-executor --test git_read_integration -- --nocapture`
- `cargo test -p keeper-git-executor --tests`
- `cargo check --workspace --all-targets`
- `cargo clippy -p keeper-git-executor --all-targets -- -D warnings`
- `cargo fmt --all --check`
- `git diff --check -- crates/git-executor`

## Exclusive write allowlist

- `crates/git-executor/src/runner.rs`
- `crates/git-executor/src/discovery.rs`
- `crates/git-executor/tests/git_read_integration.rs`
- `docs/execution/2026-08-03-wave-c4-d2br2-grok4-metadata-no-follow-repair-report.md`

No manifest/lock, other crate/app, renderer or script edits. Report blocker
rather than crossing the allowlist.

## Completion

Return changed files, RED/GREEN evidence, every no-follow read site, config
include proof, staging/HEAD state, residual processes and risks. Post one Keeper
completion message and emit `COMPLETION: complete` or a precise blocker.
Controller alone accepts.
