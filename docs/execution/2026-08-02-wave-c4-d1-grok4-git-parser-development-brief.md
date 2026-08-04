# C4-D1 Grok4 development brief — read-only Git parser foundation

Role: real Grok CLI slot `grok4`, implementation worker.

Read the three AGENTS files, the accepted C4 design, the Git/security research
reports and Task 3 of the C4 implementation plan. Boot Keeper as worker for
task `c4-d1-git-parser` before editing.

## Deliverable

Create `keeper-git-executor` as a pure parser/bounds crate. Add no child
process launch and no external dependency. Parse Git porcelain v2 `-z` byte
records for ordinary, untracked, rename, conflict, submodule and detached HEAD
states. Use portable paths, reject malformed or partial records, cap status at
5,000 entries/2 MiB and one text diff at 256 KiB. Binary and oversized input
return typed summaries rather than partial success.

## Write allowlist

- `Cargo.toml`
- `crates/git-executor/Cargo.toml`
- `crates/git-executor/src/lib.rs`
- `crates/git-executor/src/status.rs`
- `crates/git-executor/src/diff.rs`
- `crates/git-executor/tests/status_porcelain.rs`
- `crates/git-executor/tests/bounded_diff.rs`
- `docs/execution/2026-08-02-wave-c4-d1-grok4-git-parser-development-report.md`

Do not modify `Cargo.lock`; if it changes unexpectedly, stop and report before
touching it. Do not stage or commit. Run the Task 3 RED/GREEN gates and report
parser coverage, malformed cases, workspace check, changed files and risks.
End with one Keeper signal.
