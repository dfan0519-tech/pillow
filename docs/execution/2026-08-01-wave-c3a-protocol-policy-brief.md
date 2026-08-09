# Wave C3-A Brief — Protocol And Approval Kernel

Owner: grok3 implementation worker

Controller session: `<keeper-session>`

## Startup

1. Work only in `this repository`.
2. POST Keeper boot with `project=pillow`, task
   `c3a-protocol-policy`, agentName `grok3`, agentRole `worker`, and
   `<local-parent-session>`.
3. Read the three AGENTS files and
   `docs/superpowers/specs/2026-08-01-c3-governed-write-design.md`.
4. Preserve the accepted dirty tree. Do not reset, stage or commit.

## Goal

Extend the fixed provider-neutral tool contract with strict `fs.write@1`
create/modify UTF-8 arguments and create a pure `keeper-policy` approval state
machine. This task does not touch SQLite, filesystem execution, runtime,
adapter, Tauri or React.

## Exact Contract

```rust
pub enum WriteMode { Create, Modify }
pub struct FsWriteArguments {
    pub path: String,
    pub mode: WriteMode,
    pub content: String,
}
pub struct FsWriteOutput {
    pub path: String,
    pub mode: WriteMode,
    pub bytes_written: u64,
    pub content_digest: String,
}
pub enum WriteProposalState {
    PendingApproval, Approved, Applying, Succeeded, Rejected,
    Expired, Cancelled, Interrupted, Failed,
}
pub enum WriteDecision { ApproveOnce, Reject }
pub const WRITE_CONTENT_BYTE_CAP: u64 = 262_144;
pub const WRITE_PROPOSAL_TTL_SECS: i64 = 600;
```

`fs.write@1` arguments accept exactly `path`, `mode`, `content`. Content must
contain no NUL and its UTF-8 byte length must be `1..=262144`; an empty file is
not supported in C3.0. Reuse the exact existing portable path grammar. Results
are strict and bind path/mode/byte count/canonical SHA-256 digest.

Approval transitions are exactly:

```text
pending_approval -> approved -> applying -> succeeded | failed
pending_approval -> rejected | expired | cancelled | interrupted
approved         -> expired | cancelled | interrupted | applying
applying         -> succeeded | failed | interrupted
```

Terminal states never reopen. Expiry is true at `now >= expires_at`.

Add fixed canonical public error codes/messages for approval required,
rejected, expired, base changed, write conflict, path rejected and write
failed. Wire parsing must reject arbitrary messages.

## File Whitelist

- `Cargo.toml`
- `Cargo.lock`
- `crates/kernel-policy/Cargo.toml`
- `crates/kernel-policy/src/lib.rs`
- `crates/kernel-policy/src/write.rs`
- `crates/kernel-policy/tests/write_policy.rs`
- `crates/agent-protocol/src/tool/types.rs`
- `crates/agent-protocol/src/tool/manifest.rs`
- `crates/agent-protocol/src/tool/validate.rs`
- `crates/agent-protocol/src/tool/public_error.rs`
- `crates/agent-protocol/src/lib.rs`
- `crates/agent-protocol/tests/tool_contract.rs`
- `crates/agent-protocol/tests/tool_contract_security.rs`
- `docs/execution/2026-08-01-wave-c3a-protocol-policy-report.md`

Do not modify any other path. If another file is genuinely required, stop and
report rather than expanding scope.

## Required TDD And Gates

1. Add failing protocol tests first; run them and record the expected RED.
2. Implement minimal strict protocol changes; keep all read-tool tests green.
3. Add failing policy state/expiry/replay tests; record RED.
4. Implement pure policy transitions without IO or UI dependencies.
5. Run:
   - `cargo test -p keeper-agent-protocol --tests`
   - `cargo test -p keeper-policy --tests`
   - `cargo fmt --all -- --check`
   - `git diff --check`
6. Verify `git diff --cached --name-only` is empty.

## Completion Report

Write only:

`docs/execution/2026-08-01-wave-c3a-protocol-policy-report.md`

Include changed files, exact test commands/results, RED evidence, contract
decisions, residual risks, `git status --short` for the whitelist, and
`COMPLETION: DONE` or `COMPLETION: BLOCKED`.
