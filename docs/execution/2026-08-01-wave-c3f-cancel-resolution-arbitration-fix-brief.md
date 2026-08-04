# Wave C3-F Product Fix Brief — Cancel Resolution Arbitration

Owner: Grok runtime worker; controller owns review and acceptance

Worker session: `<keeper-session>`

Runtime allocation: `grok3` / `<keeper-lease>`

Status: ACTIVE. This is a narrow product fix for the C3-F cancellation blocker.

## Confirmed Defect

When Stop cancels a run with a live governed-write proposal, the write
coordinator durably resolves the proposal as cancelled, but host cancel marks
the bounded delivery and protocol guard cancelled before the adapter can emit
the corresponding `write_proposal_resolved`. The UI therefore receives
`run_cancelled` without the conversation-owned write terminal and cannot
event-drive its pending-proposal refresh.

## Required Contract

1. After host cancel wins, permit at most one narrowly validated
   `write_proposal_resolved` event whose state/error pair is exactly:
   - `cancelled` / `cancelled`, or
   - `interrupted` / `interrupted`.
2. That safe resolution must be ordered before the single host-forced
   `run_cancelled` terminal.
3. Every other post-cancel event remains rejected, including write success,
   reject, expire, failure, ordinary tool output and turn completion.
4. A malformed payload, mismatched state/error pair or duplicate safe
   resolution fails closed.
5. Do not weaken bounded-queue cancellation arbitration, sequence validation,
   one-terminal enforcement or shutdown behavior.
6. The governed-write waiter must still durably transition storage before the
   safe resolution is emitted. No target mutation may occur on cancel.

Use TDD: first add tests that reproduce the missing safe resolution and prove
that success/malformed/duplicate events remain blocked; then implement the
smallest runtime-host change.

## Write Whitelist

- `crates/runtime-host/src/delivery.rs`
- `crates/runtime-host/src/sink.rs`
- `crates/runtime-host/src/worker.rs` only if tests prove the terminal ordering
  cannot be closed in delivery/sink alone
- `crates/runtime-host/tests/governed_write_bridge.rs`
- `docs/execution/2026-08-01-wave-c3f-cancel-resolution-arbitration-fix-report.md`

Do not edit the adapter, protocol crate, native conversation service, renderer,
storage, policy, executor, C3-F harness or fake ACP fixtures. If the contract
cannot be satisfied inside this whitelist, stop and report the exact blocker.

## Gates

- focused new cancellation/arbitration tests;
- `cargo test -p keeper-runtime-host --tests -- --test-threads=1`;
- `cargo check --workspace --all-targets`;
- `cargo clippy -p keeper-runtime-host --all-targets -- -D warnings`;
- format check on changed Rust files;
- whitelist `git diff --check`;
- HEAD unchanged and staging empty.

## Completion

Write the report with RED/GREEN evidence, exact event order, negative cases,
changed files, residual risks and `COMPLETION: DONE` or `COMPLETION: BLOCKED`.
Send a completion message to controller session
`<keeper-session>`, then exit. Do not reset, restore, stage,
commit, package, publish or write to the frozen old Keeper tree.
