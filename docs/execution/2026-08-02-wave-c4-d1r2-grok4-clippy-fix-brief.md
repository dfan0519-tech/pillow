# Pillow C4 D1R2 — Git parser Clippy repair brief

## Role and operating boundary

- Worker: real Grok CLI slot `grok4`.
- Keeper task: `c4-d1r2-git-clippy-fix`; boot as `agentRole=worker` before editing.
- Repository: `this repository` only. Treat `legacy Keeper (out of tree)` as frozen and read-only.
- Preserve the accepted dirty C1–C3 working tree. Do not stage, commit, reset, clean, or overwrite unrelated changes.
- Send one completion signal when finished; do not create a polling monitor.

## Current red evidence

The controller's second exact `corepack pnpm check` passed all 268 front-end tests plus Rust tests and doctests, then failed only at Clippy with `clippy::byte-char-slices`:

- `crates/git-executor/src/status.rs:401`: `[b'?', b'?']`
- `crates/git-executor/src/status.rs:402`: `[b'!', b'!']`

Clippy recommends `*b"??"` and `*b"!!"`. This is the only authorized production change.

## Required workflow

1. Read `root AGENTS.md`, `shared vault rules (out of tree)`, `this repository\AGENTS.md`, this brief, and the D1/D1R reports relevant to the Git parser.
2. Reproduce RED with:
   `cargo clippy -p keeper-git-executor --all-targets -- -D warnings`
3. Make the minimal two-expression repair in `status.rs`; do not refactor or change parser behavior.
4. Verify GREEN with:
   - `cargo test -p keeper-git-executor`
   - `cargo clippy -p keeper-git-executor --all-targets -- -D warnings`
   - `cargo fmt --all --check`
   - `git diff --check -- crates/git-executor/src/status.rs`
5. Write the report listed below with RED/GREEN evidence, changed files, residual risk, and explicit completion status.

## Exclusive write allowlist

- `crates/git-executor/src/status.rs`
- `docs/execution/2026-08-02-wave-c4-d1r2-grok4-clippy-fix-report.md`

No other file may be changed. In particular, do not modify `Cargo.lock`, manifests, tests, plans, or existing reports.

## Completion contract

Report:

- exact changed files;
- exact commands and exit states;
- confirmation that the initial Clippy command was observed failing for the expected lint;
- remaining risks;
- `COMPLETION: complete` or a precise blocker.

The controller, not the worker, owns the full `pnpm check`, production build, final diff review, and acceptance decision.
