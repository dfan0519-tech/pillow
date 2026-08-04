# Wave C3-F Product Fix Brief — Provider Cancel RPC Completion

Owner: Grok adapter worker; controller owns review and acceptance

Worker session: `<keeper-session>`

Runtime allocation: `grok3` / `<keeper-lease>`

Status: ACTIVE. Narrow fix for the post-cancel next-turn hang.

## Confirmed Root Cause

On a pending governed write, cancel durably produces a cancelled/interrupted
receipt. The accepted runtime-host arbitration commits that safe
`write_proposal_resolved`, then forces `run_cancelled` and intentionally returns
`RuntimeError::Cancelled` to the emitter. In `handle_write_tool`, `sink.emit`
uses `?`, so this expected post-cancel signal exits before the adapter sends the
fixed error response to the original `pillow/write_tool` server request. The
provider child keeps waiting for that request id and the next same-process
session/turn stalls.

## Required Contract

1. If and only if all of the following hold, treat `sink.emit` returning
   `RuntimeError::Cancelled` as an already-committed safe cancel resolution and
   continue provider completion:
   - the turn cancel token is set;
   - the governed-write receipt is `Executed`;
   - the resolved state/error pair is exactly cancelled/cancelled or
     interrupted/interrupted.
2. Respond exactly once to the original `pillow/write_tool` request with its
   existing fixed provider error code/message. Never return success and never
   include path/content/grant/debug details.
3. Every other sink error, mismatched receipt or non-cancel state continues to
   propagate/fail closed.
4. The prompt loop must finish cleanup: no live turn, event sink cleared,
   provider request no longer pending, operation gate released. Do not rely on
   the 180-second request timeout.
5. Prove with the same adapter instance that cancel-while-pending is followed
   by a fresh session/turn completing within a tight deterministic budget.
6. Preserve exactly one `write_proposal_resolved` followed by exactly one
   `run_cancelled`; no target mutation on cancel.

Use TDD: first reproduce the provider request remaining unanswered / next turn
stall, then implement the smallest adapter-side fix.

## Write Whitelist

- `crates/adapter-grok-acp/src/prompt.rs`
- `crates/adapter-grok-acp/tests/adapter_fake_child.rs`
- `crates/adapter-grok-acp/tests/governed_write_roundtrip.rs`
- `crates/adapter-grok-acp/tests/live_native_acceptance.rs` only if the same-
  process recovery contract belongs there
- `crates/adapter-grok-acp/src/bin/fake_acp_agent/scripts.rs` only for the
  smallest deterministic cancel-then-next-turn scenario
- `docs/execution/2026-08-01-wave-c3f-provider-cancel-rpc-completion-fix-report.md`

Do not edit runtime-host, protocol, native conversation service, renderer,
storage, policy, executor or C3-F harness. If cleanup cannot be proven within
this boundary, stop and report why.

## Gates

- focused RED/GREEN cancel-then-next-turn tests;
- `cargo test -p keeper-adapter-grok-acp --tests -- --test-threads=1`;
- `cargo check --workspace --all-targets`;
- `cargo clippy -p keeper-adapter-grok-acp --all-targets -- -D warnings`;
- `cargo fmt -p keeper-adapter-grok-acp -- --check`;
- whitelist `git diff --check`;
- staging empty and HEAD unchanged.

## Completion

Write the report with the exact provider response, event order, cleanup proof,
RED/GREEN evidence, changed files, residual risks and `COMPLETION: DONE` or
`COMPLETION: BLOCKED`. Signal controller session
`<keeper-session>`, then exit. Do not reset, restore, stage,
commit, package, publish or write to the frozen old Keeper tree.
