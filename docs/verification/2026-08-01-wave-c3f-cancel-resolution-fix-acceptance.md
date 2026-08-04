# Pillow Wave C3-F Cancel Resolution Fix Acceptance

Date: 2026-08-01

Verdict: ACCEPT

Scope: preserve the conversation-owned cancelled/interrupted write outcome
when host cancel wins, without reopening general post-cancel delivery.

## Accepted Contract

- At most one validated `write_proposal_resolved` with an exact
  cancelled/cancelled or interrupted/interrupted state/error pair may commit
  after host cancel.
- The safe write resolution is sequenced before the sole host-forced
  `run_cancelled` terminal.
- Success, rejection, expiry, failure, malformed/mismatched payloads,
  duplicates, ordinary output and turn completion remain blocked.
- Delivery capacity, cancellation arbitration, sequence validation,
  one-terminal behavior and shutdown semantics remain intact.
- The host does not fabricate a resolution; the governed-write path still
  owns the durable transition and emitted receipt.

## Fresh Controller Evidence

- Complete `keeper-runtime-host` test inventory: 74/74 passed, including six
  focused unit cases and two integration arbitration cases.
- `cargo check --workspace --all-targets`: passed.
- `cargo clippy -p keeper-runtime-host --all-targets -- -D warnings`: passed.
- `cargo fmt -p keeper-runtime-host -- --check`: passed.
- Whitelist `git diff --check`: passed.
- HEAD remains `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`; staging is empty.

## Boundary

This acceptance closes the runtime cancellation blocker only. The final proof
remains the real Tauri/WebView2 C3-F cancellation scenario and complete C3-F
gate set. It does not authorize staging or commit.
