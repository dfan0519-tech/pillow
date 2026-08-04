# C4-D1R Codex2 development brief — Rust integration corrections

Role: real account-2 Codex CLI (`CODEX_HOME=%USERPROFILE%\.codex2`),
`gpt-5.6-terra`, reasoning `high`, implementation worker.

Boot Keeper as a worker for project `pillow`, task
`c4-d1r-rust-integration` before editing. Read the three AGENTS files, the
accepted C4 design, Tasks 1–3 of the C4 implementation plan, and all four D1
development reports. Preserve the accepted dirty C1–C3 tree. Do not stage or
commit.

## Controller findings to correct with TDD

1. `process.run` has a `Vec<String>` argv but its manifest advertises a scalar
   string. Add an exact additive manifest argument kind for a string array and
   lock it in contract tests.
2. Re-export `ProcessRunArguments`, `ProcessRunOutput`,
   `PROCESS_ARGV_COUNT_CAP`, and `PROCESS_ARGV_BYTE_CAP` through `tool/mod.rs`
   and the crate root. Replace the tautological constants test with imports of
   the real constants.
3. The existing read-only `ReadToolExecutor` must explicitly fail closed for
   `ToolArguments::ProcessRun` without spawning, just as writes fail closed.
   Add a focused regression proving the result is approval-required and no
   child process path exists.
4. Harden the pure Git parser: validate `GitRepositoryId.project_relative`
   through the same portable grammar at parser ingress, and reject empty NUL
   records rather than silently skipping malformed input. Add focused tests.
5. Intentionally update `Cargo.lock` for the new zero-dependency
   `keeper-git-executor` workspace member. The lock diff must be minimal and
   reviewed; do not refresh unrelated packages.

## Write allowlist

- `Cargo.lock`
- `crates/agent-protocol/src/tool/types.rs`
- `crates/agent-protocol/src/tool/manifest.rs`
- `crates/agent-protocol/src/tool/mod.rs`
- `crates/agent-protocol/src/lib.rs`
- `crates/agent-protocol/tests/tool_contract.rs`
- `crates/agent-protocol/tests/tool_contract_security.rs`
- `crates/tool-executor/src/lib.rs`
- `crates/tool-executor/tests/read_executor.rs`
- `crates/git-executor/src/status.rs`
- `crates/git-executor/tests/status_porcelain.rs`
- `docs/execution/2026-08-02-wave-c4-d1r-codex2-rust-integration-report.md`

Do not modify any other file. Do not implement the real process executor,
storage or gateway. Run RED tests for each finding, then package tests,
`cargo check --workspace --all-targets`, `cargo test --workspace --all-targets`,
`cargo fmt --all --check`, and scoped `git diff --check`. Report exact changed
files, RED/GREEN evidence, lock delta, residual risk/processes, staging state,
and end with one completion or blocker signal.
