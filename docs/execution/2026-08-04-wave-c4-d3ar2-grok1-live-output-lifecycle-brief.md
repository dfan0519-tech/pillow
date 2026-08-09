# C4 D3AR2 Grok1 brief — real live output and retained terminal projection

Read this file first. It is the complete task contract.

You are the actual account-2 Codex CLI worker (`CODEX_HOME=%USERPROFILE%\.codex2`,
`gpt-5.6-terra`, high reasoning, default service tier / fastMode=false).
Repair only the rejected D3AR output lifecycle. Boot Keeper as a `pillow` worker for task
`c4-d3ar2-live-output-lifecycle`. Work only in `this repository`; Keeper is
read-only. Preserve HEAD `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`,
empty staging and all unrelated dirty work. No stage, commit, reset, restore,
clean, package, publish or numeric-PID/name cleanup.

## Rejection being repaired

The current `OutputObserver` receives raw bytes before byte/line caps and
neutralization. `GovernedProcessToolGateway::execute` does not expose an
observer seam to runtime-host. Runtime-host therefore leaves `output_chunks`
empty while the command runs, receives output only after terminal completion,
and then its waiter removes the live entry. UI output is empty while running
and racy/unavailable after completion.

## Required behavior

1. The supervised runner's observer receives only already-retained bounded
   output chunks: stream tag, monotonic sequence, lossy UTF-8 marker and
   control-neutralized display text. It must never receive bytes beyond the
   combined 256 KiB / 4,000-line limits. Raw unbounded `&[u8]` must not cross
   the observer boundary.
2. Add a narrow governed gateway execution-with-observer path while keeping the
   ordinary `execute` wrapper. The observer is data-only/non-authoritative;
   claim, spawn, cancellation and terminal authority remain unchanged.
3. Runtime-host passes an owner/conversation/proposal-bound observer. While a
   slow process is still running, `open_process_output` must expose emitted
   chunks without waiting for terminal completion.
4. After terminal completion and waiter receipt consumption, output remains in
   a host-owned bounded retained-output cache so the same owner/conversation
   can reopen it. Retain at most 32 terminal proposal outputs per host owner
   (maximum 8 MiB at the existing per-process cap); evict oldest first.
5. Individual run release must not immediately erase terminal output. Host
   shutdown/drop/project-host replacement clears the retained cache. A fresh
   host/restart never reconstructs transcript text from SQLite.
6. When durable terminal metadata exists but live text was not retained, the
   owner-bound open path returns an empty `ProcessOutputView` with
   `live_retained=false` and terminal metadata, not a fake transcript. Foreign
   project/conversation/proposal access still fails closed.
7. Preserve all D2A/D2B process safety, one-shot execution, grants, event
   payloads and SQLite metadata-only rules.

## Exact write allowlist

- `crates/process-executor/src/lib.rs`
- `crates/process-executor/src/output.rs`
- `crates/process-executor/tests/supervised_process.rs`
- `crates/tool-executor/src/process.rs`
- `crates/tool-executor/src/lib.rs`
- `crates/tool-executor/tests/governed_process.rs`
- `crates/runtime-host/src/host/process.rs`
- `crates/runtime-host/src/host/mod.rs`
- `crates/runtime-host/src/lib.rs`
- `crates/runtime-host/tests/governed_process_bridge.rs`
- `docs/execution/2026-08-04-wave-c4-d3ar2-codex2-live-output-lifecycle-report.md`

Do not touch protocol, kernel-session, storage, policy, Tauri/TS, Git, root
manifests/lockfile, roadmap/progress or the parallel event-durability worker's
files.

## Required tests and gates

Use TDD. Prove raw-over-cap bytes never reach observer; live slow-command output
is visible before terminal; stdout/stderr ordering and lossiness survive;
terminal output reopens after waiter completion and run release; 33rd retained
output evicts the oldest; shutdown/restart clears text; durable-only terminal
returns `live_retained=false`; foreign access fails; cancellation/timeout and
zero residual authority remain green.

Run:

```text
cargo test -p keeper-process-executor --all-targets
cargo test -p keeper-tool-executor --all-targets
cargo test -p keeper-runtime-host --all-targets
cargo test -p keeper-runtime-host --features test-support --test governed_process_bridge
cargo check -p keeper-process-executor -p keeper-tool-executor -p keeper-runtime-host --all-targets
cargo clippy -p keeper-process-executor -p keeper-tool-executor -p keeper-runtime-host --all-targets -- -D warnings
cargo fmt --all --check
git diff --check -- <allowlisted source and test paths>
git diff --cached --name-only
git rev-parse HEAD
```

Write the exact report path with changed files, RED/GREEN evidence, commands,
risks, HEAD/staging and residual-process evidence. End with
`COMPLETION: complete` only if every requirement is met; otherwise report
`BLOCKED` or `DONE_WITH_CONCERNS`. Controller acceptance is separate.
