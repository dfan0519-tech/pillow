# Pillow C4 D2BR - Git containment and fail-closed repair brief

## Worker identity and session

- Worker: real Grok CLI slot `grok4`, model Grok 4.5, high reasoning.
- Keeper boot: `project=pillow`, `task=c4-d2br-git-containment-repair`,
  `agentName=grok4`, `agentRole=worker`,
  `parentSessionId=<keeper-session>`.
- Repository: `<workspace>\pillow` only. Keeper and references are read-only.
- Preserve HEAD `7b7f70f` and every existing dirty C1-C4 slice. No stage,
  commit, reset, restore, clean, package or publish.

## Why D2B was rejected

The controller's focused tests were green, but live Git output and source
review proved the executable-configuration probe is ineffective and multiple
filesystem paths can cross the project authority boundary. These are security
acceptance blockers, not optional hardening.

## Required repairs

1. Make unsafe-configuration discovery fail closed. A spawn error, non-zero
   exit, truncated output, invalid UTF-8 or unparseable `git config` result must
   return a typed denial/error, never be treated as safe.
2. Parse the real `git config --local --list --null` wire form used by Git:
   `key\nvalue\0`. Add a real fixture containing unsafe include, hook/filter,
   textconv or executable config. Only tolerate another form if explicitly
   documented and tested; never silently ignore malformed records.
3. Before any untracked `git diff --no-index` or file read, resolve every path
   component without following an unsafe link. Reject symlinks and Windows
   reparse points/junctions, canonicalize the final file, and prove it remains
   under the active project/worktree authority boundary.
4. Discovery traversal must reject Windows `FILE_ATTRIBUTE_REPARSE_POINT`, not
   only Rust `is_symlink()`. Never traverse an outside junction while looking
   for repositories.
5. A `.git` file whose resolved gitdir/common dir leaves the project-owned
   worktree boundary is unavailable. Do not read outside config, hooks or
   attributes. Internal linked-worktree metadata may be accepted only when its
   real paths remain inside the project boundary.
6. Every diff response must carry repository identity plus the supplied
   generation. Do not ignore the `generation` parameter. Use a bounded typed
   snapshot/wrapper so later native/renderer code can reject stale results.
7. Do not parse or publish lossy status/diff output. Invalid UTF-8, truncation
   or output uncertainty must map to a typed fail-closed/unsupported result.
8. Scan `info/attributes` from the validated actual gitdir for accepted
   internal linked worktrees; do not assume `workdir/.git` is a directory.
9. Preserve fixed read-only Git templates, clean environment, prompt/pager/
   ext-diff/textconv suppression, output bounds and the no-mutation boundary.

## Required RED -> GREEN evidence

Add tests that fail before the repair and pass after it for:

- exact `key\nvalue\0` unsafe-config detection;
- config probe process failure, non-zero, truncation, invalid UTF-8 and malformed
  records all fail closed;
- untracked diff through a symlink and a Windows junction/reparse point cannot
  read outside content;
- discovery cannot traverse a project-contained junction to an outside repo;
- `.git` file resolving outside the project-owned boundary is unavailable;
- accepted internal linked-worktree `info/attributes` is inspected;
- status/diff invalid UTF-8 and truncation fail closed;
- diff snapshot contains repository identity and the exact input generation;
- existing ordinary, rename, binary, conflict, detached, bounded diff and
  fixed-template tests remain green.

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
- `crates/git-executor/src/diff.rs`
- `crates/git-executor/src/lib.rs`
- `crates/git-executor/tests/git_read_integration.rs`
- `crates/git-executor/tests/bounded_diff.rs`
- `docs/execution/2026-08-03-wave-c4-d2br-grok4-git-containment-repair-report.md`

Do not modify any manifest/lock, protocol, policy, process-executor, storage,
tool-executor, runtime-host, Tauri, renderer or scripts. Stop and report if the
repair genuinely requires an out-of-allowlist change.

## Completion contract

Report exact changed files, before/after tests, real config-wire evidence,
containment proof, generation contract, staging state, residual processes and
risks. Post one Keeper completion message to the parent session and emit
`COMPLETION: complete` or a precise blocker. The controller alone accepts.
