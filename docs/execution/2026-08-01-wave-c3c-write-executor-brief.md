# Wave C3-C Brief — Host Diff And Atomic Write Executor

Owner: grok3 implementation worker

Controller session: `<keeper-session>`

Activated after controller accepted C3-A and C3-B. Fresh C3-B controller gates:
write repository 23/23, migrations 4/4, full storage suite, workspace all-targets
check, fmt and diff checks all passed; staging remained empty.

## Startup And Inputs

Boot Keeper as worker `grok3` with task `c3c-write-executor` and the controller
parent session. Read AGENTS, the C3 design, and the accepted C3-A/C3-B reports.
Do not reset, stage or commit.

## Goal

Implement a pure host-side write preparation/diff/apply layer. It receives a
sealed active payload from the runtime host, verifies it against durable
metadata, and writes only after the repository already contains an accepted
one-shot decision.

## Required Interfaces

```rust
pub struct BoundedDiff {
    pub unified: String,
    pub truncated: bool,
    pub line_count: u32,
    pub byte_count: u64,
}

pub struct PreparedWriteProposal {
    pub metadata: StoredWriteProposal,
    pub base_content: Option<String>,
    pub proposed_content: String,
    pub diff: BoundedDiff,
}

pub struct GovernedWriteToolGateway;
pub fn build_bounded_unified_diff(base: Option<&str>, proposed: &str) -> BoundedDiff;
```

Use the accepted typed storage APIs (`WriteApplyReceipt`,
`WriteApplyErrorCode`, `WriteApplyReconcileObservation`) rather than accepting
or constructing free-form receipt JSON.

The diff limit is 262,144 bytes and 4,000 lines. Proposed/base content is never
written to SQLite, logs or run events.

## Exact Write Policy

- Existing portable path grammar remains mandatory.
- Deny components `.git`, `.pillow`, `.keeper`, `.ssh`.
- Deny leaves `.env`, `.env.local`, `id_rsa`, `id_ed25519`.
- Deny extensions `.pem`, `.key`, `.p12`, `.pfx`, `.jks`, `.keystore`.
- Allow `.env.example`.
- Create: target absent, existing real parent directory.
- Modify: existing regular UTF-8 file, at most 262,144 bytes.
- Recheck base digest immediately before replacement.
- Same-directory random `create_new` temp; write, flush, `sync_all`.
- On Windows, create uses `MoveFileExW` without replace-existing and with
  write-through semantics, so a target created after the final absence check
  is never overwritten. Modify uses `ReplaceFileW` with write-through semantics.
  Never remove/unlink the target before either operation and never fall back to
  a copy or truncate-in-place path.
- Revalidate root, every existing parent component, target kind/reparse state,
  mode condition and base digest immediately before the OS atomic operation.
- Pre-replace failure cleans temp. Post-replace success digest is persisted
  before return.
- Sealed base/proposed content must not implement an exposing `Debug`,
  `Serialize` or logging path. If `Debug` is required, redact both contents.
- Diff construction must be deterministic and bounded in time/memory; do not
  introduce an unbounded quadratic line-diff algorithm for 262,144-byte input.

## File Whitelist

- `crates/tool-executor/Cargo.toml`
- `Cargo.lock` only for the exact `keeper-tool-executor` dependency edge needed
  by the Windows atomic primitive; no version refresh or unrelated churn
- `crates/tool-executor/src/lib.rs`
- `crates/tool-executor/src/write.rs`
- `crates/tool-executor/src/write_diff.rs`
- `crates/tool-executor/tests/governed_write.rs`
- `crates/tool-executor/tests/write_diff.rs`
- `docs/execution/2026-08-01-wave-c3c-write-executor-report.md`

## Required TDD

Write RED tests first for create/modify, create-existing, modify-missing,
oversize/binary base, base drift, every denied path class, traversal/device/ADS,
file/directory/symlink/reparse, cancellation, concurrent apply, temp cleanup,
Unicode/final-newline diff and exact truncation. Record the RED command/result.

Run the complete tool-executor tests plus C3 protocol/storage regressions,
`cargo fmt --all -- --check`, `git diff --check`, and staging-empty check.

## Completion

Write only `docs/execution/2026-08-01-wave-c3c-write-executor-report.md` with
changed files, RED/GREEN evidence, Windows atomic primitive, cleanup evidence,
risks and `COMPLETION: DONE` or `COMPLETION: BLOCKED`.
