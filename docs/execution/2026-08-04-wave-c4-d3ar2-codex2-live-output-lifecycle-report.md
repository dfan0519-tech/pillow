# C4 D3AR2 live output lifecycle repair

Date: 2026-08-04
Worker: Codex account-2 (`codex2`)
Keeper session: `<keeper-session>`
Task: `c4-d3ar2-live-output-lifecycle`

## Delivered

- Replaced the raw-byte observer contract with `OutputChunk`: stream, monotonic sequence, lossy UTF-8 marker, and neutralized display text. The observer receives only data that passed the combined byte and display-line retention path.
- Added `GovernedProcessToolGateway::execute_with_observer`; `execute` remains its ordinary wrapper. Claim, spawn, cancellation, and terminal persistence remain in the same path.
- Runtime-host installs an owner/conversation/proposal-bound observer, allowing `open_process_output` to return slow-process chunks before terminal completion.
- Terminal waiter consumption moves output into a host-owner-scoped `VecDeque`, retaining 32 terminal proposal outputs per owner and evicting the oldest first. Individual run release retains this cache; shutdown/drop clears it.
- Owner-bound durable terminal fallback returns terminal metadata with empty chunks and `live_retained=false`; transcript text is never reconstructed from SQLite.

## TDD evidence

RED anchors added before implementation:

- process-executor observer over-cap/no-raw test;
- runtime-host slow live-output/reopen test;
- runtime-host 33rd-entry oldest eviction test;
- runtime-host restart durable-only empty projection assertion.

GREEN evidence:

- `observer_never_receives_raw_over_cap_bytes` proves observer chunks stay within a 4 KiB combined retained run and control text is neutralized.
- `slow_output_is_visible_live_and_reopens_after_waiter_consumes_terminal` proves live projection, terminal retention, and post-run-release reopening.
- `terminal_output_cache_evicts_oldest_after_thirty_two_entries` proves the 33rd terminal output evicts the oldest; evicted output returns metadata-only projection while newest transcript remains retained.
- Existing cancellation, timeout, restart, foreign-access, one-shot claim and zero-residual-authority cases remain green.

## Changed files

- `crates/process-executor/src/output.rs`
- `crates/process-executor/tests/supervised_process.rs`
- `crates/tool-executor/src/process.rs`
- `crates/runtime-host/src/host/process.rs`
- `crates/runtime-host/src/host/mod.rs`
- `crates/runtime-host/tests/governed_process_bridge.rs`
- this report

## Gates

| Command | Result |
| --- | --- |
| `cargo test -p keeper-process-executor --all-targets` | PASS (3 unit + 13 supervised) |
| `cargo test -p keeper-tool-executor --all-targets` | PASS |
| `cargo test -p keeper-runtime-host --all-targets` | PASS |
| `cargo test -p keeper-runtime-host --features test-support --test governed_process_bridge` | PASS (16) |
| `cargo check -p keeper-process-executor -p keeper-tool-executor -p keeper-runtime-host --all-targets` | PASS |
| `cargo clippy -p keeper-process-executor -p keeper-tool-executor -p keeper-runtime-host --all-targets -- -D warnings` | PASS |
| `cargo fmt --all --check` | BLOCKED by installed formatter parsing pre-existing Rust-2024 `let` chains in non-allowlisted `tool-executor/src/write.rs`, `runtime-host/src/governed_write.rs`, and existing process-host code; it reports formatting diffs but cannot parse/write the workspace. |
| `git diff --check -- <allowlist>` | PASS |

## Repository and process evidence

- HEAD: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b` (preserved).
- Staging is empty (`git diff --cached --name-only`); no staging action was performed.
- No PID/name cleanup was run. All supervised cancellation/timeout tests completed with their cleanup/zero-authority assertions green; no worker-owned residual authority remains.

## Risks

- The host-wide coordinator is an existing static coordinator design; cache eviction is keyed by host owner and cleared on owner release, so a fresh host cannot see prior text.
- The formatter toolchain is older than the workspace syntax; the required `cargo fmt --all --check` gate cannot pass without changing toolchain or unrelated Rust-2024 source, both outside this task's authority.

**DONE_WITH_CONCERNS**
