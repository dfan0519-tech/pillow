# Pillow C4 D2B - hardened read-only Git runner development brief

## Worker identity and session

- Worker: real Grok CLI slot `grok4`, model Grok 4.5, high reasoning.
- Keeper boot: `project=pillow`, `task=c4-d2b-hardened-git-runner`,
  `agentName=grok4`, `agentRole=worker`,
  `<local-parent-session>`.
- Repository: `this repository` only. Keeper and references are read-only.
- Preserve HEAD `7b7f70f` and the accepted dirty C1-C3/D1/D2A tree. No stage,
  commit, reset, restore, clean, package or publish.

## Inputs to read completely

1. All applicable AGENTS files.
2. `docs/superpowers/specs/2026-08-02-c4-governed-process-git-design.md`.
3. Task 9 in `docs/superpowers/plans/2026-08-02-c4-governed-process-git-implementation.md`.
4. Accepted Git parser and shared process-executor code/reports, including the
   D2AR process-order repair report.
5. `docs/research/2026-08-02-c4-git-governance-research.md` and
   `docs/research/2026-08-02-c4-security-acceptance-research.md`.

## Goal and locked boundary

Implement `GitReadExecutor::discover`, `status` and `diff` with fixed internal
Git command templates over `SupervisedProcessRunner`. Resolve one trusted
`git.exe` from host configuration; do not expose an arbitrary argv API. Use a
clean environment, disable prompt and pager, use porcelain v2 `-z`,
`--no-ext-diff`, `--no-textconv` and the accepted output/status/diff bounds.

Reject repositories or configuration that can execute local programs,
including unsafe hooks, filters, textconv or related executable config. Reject
linked worktrees whose real path leaves the active project root. All returned
paths remain portable project-relative values. Git mutation, commit, checkout,
restore, network and credentials are out of scope.

## Required RED -> GREEN evidence

Create temporary real repositories and add tests before implementation for:

- non-repository discovery and ordinary repository discovery;
- nested repository isolation and outside-root linked-worktree denial;
- ordinary/untracked/rename/binary/conflict/detached status;
- fixed bounded diff, oversized/binary summaries and generation drift;
- prompt/pager/ext-diff/textconv suppression;
- unsafe hook/filter/textconv/executable config denial;
- trusted executable resolution failure and typed process failures;
- no mutation and no arbitrary caller-controlled Git argv.

Run:

- `cargo test -p keeper-git-executor --test git_read_integration -- --nocapture`
- `cargo test -p keeper-git-executor --tests`
- `cargo check --workspace --all-targets`
- `cargo clippy -p keeper-git-executor --all-targets -- -D warnings`
- `cargo fmt --all --check`
- `git diff --check -- crates/git-executor`

## Exclusive write allowlist

- `crates/git-executor/Cargo.toml`
- `crates/git-executor/src/runner.rs`
- `crates/git-executor/src/discovery.rs`
- `crates/git-executor/src/lib.rs`
- `crates/git-executor/tests/git_read_integration.rs`
- `docs/execution/2026-08-03-wave-c4-d2b-grok4-hardened-git-runner-report.md`

Do not modify workspace `Cargo.toml`, `Cargo.lock`, protocol, policy,
process-executor, storage, tool-executor, runtime-host, Tauri, renderer or
scripts. Stop and report if compilation genuinely requires an out-of-allowlist
change.

## Completion contract

Report exact fixed Git templates, every configuration denial, changed files,
RED/GREEN outputs and counts, staging state, residual processes and risks. Post
one Keeper completion message to the parent session and emit
`COMPLETION: complete` or a precise blocker. The controller owns acceptance.
