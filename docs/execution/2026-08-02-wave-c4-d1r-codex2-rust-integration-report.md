# C4-D1R Rust integration corrections — development report

Date: 2026-08-02

Status: development complete; not controller acceptance

## Scope

This corrective pass preserved the existing C1-C3 dirty tree and did not add a
process executor, storage, gateway, Git command runner, stage, or commit.

Changed by this pass (all within the worker allowlist):

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
- this report

`crates/git-executor` was already an untracked D1 foundation before this pass;
only its status parser and parser tests were changed here.

## Corrections

1. Added the additive `ToolArgumentType::StringArray` wire value
   `string_array`, and use it for `process.run@1.argv`. The exact manifest
   contract test now locks `program: string`, `argv: string_array`, and
   `cwd: project_relative_path`.
2. Re-exported `ProcessRunArguments`, `ProcessRunOutput`,
   `PROCESS_ARGV_COUNT_CAP`, and `PROCESS_ARGV_BYTE_CAP` through both
   `tool` and the crate root. Contract/security tests import the real caps;
   the former tautological literals were removed.
3. Made `ReadToolExecutor` explicitly return the canonical
   `approval_required` public failure for `ToolArguments::ProcessRun`, before
   any child-process route exists. The regression asserts failed state,
   approval-required code, absent output, and an unchanged empty project
   directory.
4. The pure porcelain parser now validates
   `GitRepositoryId.project_relative` with `validate_portable_path` at parser
   ingress and treats an empty NUL-delimited record as malformed. Tests cover
   empty, traversal, native-path, backslash, and Windows-device repository
   identities plus an empty record between valid headers.

## TDD evidence

RED commands and observed failures:

```text
cargo test -p keeper-agent-protocol --test tool_contract -- --nocapture
  -> unresolved crate-root ProcessRunArguments, ProcessRunOutput, and cap exports

cargo test -p keeper-tool-executor --test read_executor \
  process_run_fails_closed_with_approval_required_without_a_child_process_path -- --nocapture
  -> non-exhaustive ReadToolExecutor match: ToolArguments::ProcessRun not covered

cargo test -p keeper-git-executor --test status_porcelain \
  rejects_invalid_repository_identity_at_parser_ingress -- --nocapture
  -> parser accepted invalid repository identity

cargo test -p keeper-git-executor --test status_porcelain \
  rejects_empty_nul_delimited_records -- --nocapture
  -> parser accepted empty NUL record
```

GREEN evidence:

```text
cargo test -p keeper-agent-protocol --tests
  -> workspace rerun included protocol unit 14, contract 40, security 18
cargo test -p keeper-tool-executor --tests
  -> read executor 10/10, with package regressions included by workspace rerun
cargo test -p keeper-git-executor --tests
  -> bounded diff 6/6; porcelain parser 15/15
cargo check --workspace --all-targets
  -> PASS
cargo test --workspace --all-targets
  -> PASS (one existing opt-in live ACP test ignored)
```

The first combined broad command exceeded its 120-second command limit during
cold compilation. The subsequent standalone workspace check and complete
workspace test rerun both reached successful terminal status without altering
test scope or timeouts.

## Lock review

`Cargo.lock` changed by 68 added lines and no removed lines. It adds the
zero-dependency `keeper-git-executor` package entry and the exact local
workspace package/dependency edges needed to bring the already-present C1-D1
workspace manifests into the lock. No registry package version, checksum, or
source was refreshed or changed.

## Residual risk and state

This does not implement real process execution: `process.run` remains
approval-required in the read executor and has no child-spawn path. Git remains
pure parsing only. No relevant `pillow`, `git`, `cargo`, or `rustc` process was
left running after the test gates. Cached diff was empty; nothing was staged or
committed. C4 acceptance remains controller-only.
