# Pillow Wave C3-F Provider Cancel RPC Fix Acceptance

Date: 2026-08-01

Verdict: ACCEPT

Scope: complete the original provider write-tool RPC and release adapter prompt
state after host cancel has already committed the safe write resolution.

## Accepted Contract

- Only an executed receipt with a set cancel token and an exact
  cancelled/cancelled or interrupted/interrupted pair may continue after the
  resolution emitter returns `RuntimeError::Cancelled`.
- The original provider request receives exactly one fixed non-success error;
  cancelled uses `-32042 / governed_write_cancelled`.
- Other sink failures and mismatched/non-cancel receipts still propagate.
- The prompt turn and event sink are cleared after the host-forced terminal,
  so the adapter does not retain `acp_busy` and no 180-second timeout is needed.
- Cancel does not mutate the target; event order remains safe resolution then
  one `run_cancelled`.

## Fresh Controller Evidence

- Focused cancel-then-next-turn integration: passed in 0.83 seconds.
- Complete adapter test inventory passed: unit 59, adapter child 19,
  governed read 3, governed write 4, runtime child 6, live diagnostics 4 with
  one opt-in ignored, protocol fixtures 3.
- `cargo check --workspace --all-targets`: passed.
- `cargo clippy -p keeper-adapter-grok-acp --all-targets -- -D warnings`:
  passed.
- `cargo fmt -p keeper-adapter-grok-acp -- --check`: passed.
- Whitelist `git diff --check`: passed.
- HEAD remains `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`; staging is empty.

## Boundary

This acceptance closes the same-process provider recovery blocker. C3-F still
requires a fresh complete real Tauri/WebView2 run and all repository gates. It
does not authorize staging or commit.
