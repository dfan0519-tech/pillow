# Wave C3-A Controller Rejection Fix Brief

Owner: grok3 implementation worker
Controller: Codex Desktop
Date: 2026-08-01

## Startup

- Work only in `<workspace>\pillow`.
- Worker Keeper session: `<keeper-session>`.
- Read `<workspace>\AGENTS.md`, `<workspace>\VaultSystem\vault\AGENTS.md`, and
  `<workspace>\pillow\AGENTS.md` before editing.
- Preserve every pre-existing dirty-tree change. The frozen legacy manager at
  `<workspace>\VaultSystem\keeper` is read-only and out of scope.
- This is a correction pass on the current shared working tree; do not use or
  create a Git worktree.

## Why C3-A Is Rejected

Controller acceptance command `cargo check --workspace --all-targets` fails at
`crates/tool-executor/src/lib.rs:115`: `ToolArguments::FsWrite` is not handled.
No C3 wave may leave the accepted C1/C2 workspace uncompilable.

The controller also found that the new 1 MiB canonical-argument cap is not
enough for a valid 262,144-byte UTF-8 write whose control characters require
six-byte JSON escaping. The documented `fs.write@1` byte boundary must work for
all allowed non-NUL UTF-8 content, not only ordinary source text.

## Allowed Files

- `crates/tool-executor/src/lib.rs`
- `crates/tool-executor/tests/**` only if an existing integration-test file is
  the natural home; prefer the current unit-test module when available
- `crates/agent-protocol/src/tool/digest.rs`
- `crates/agent-protocol/tests/tool_contract.rs`
- `crates/agent-protocol/tests/tool_contract_security.rs`
- `docs/execution/2026-08-01-wave-c3a-protocol-policy-report.md`

No other path may be edited. Stop and report if another path is required.

## Required Corrections

1. Add an explicit fail-closed `FsWrite` arm to the existing read executor.
   It must perform no filesystem mutation and return a canonical
   `ApprovalRequired` failed result. Add a regression test proving both the
   public outcome and that the target file is not created or changed.
2. Replace the misleading “modest escaping” cap/rationale with a still-bounded
   cap that covers the worst-case sixfold JSON escaping of the full write
   content plus the already-bounded path/schema overhead. Add a test using
   exactly `WRITE_CONTENT_BYTE_CAP` bytes of a non-NUL control character that
   serializes as `\\u00XX`; it must validate. Keep the one-byte-over-content-cap
   rejection test.
3. Update the C3-A report with the rejection, exact fix, and fresh evidence.

## Mandatory Tests

```text
cargo test -p keeper-agent-protocol --tests
cargo test -p keeper-policy --tests
cargo test -p keeper-tool-executor --all-targets
cargo check --workspace --all-targets
cargo fmt --all -- --check
git diff --check
git diff --cached --name-only
```

The completion report must include changed files, exact results, residual risk,
and end with `## COMPLETION: DONE`. Do not stage, commit, reset, or discard.
