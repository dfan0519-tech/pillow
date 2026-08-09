# Pillow C4 D3A - runtime-host process coordination brief

## Worker and authority

- Worker: actual Grok CLI `.grok1`, Grok 4.5 high.
- Keeper boot: `project=pillow`, `task=c4-d3a-runtime-host-process`,
  `agentName=grok1`, `agentRole=worker`, parent
  `<keeper-session>`.
- Work only in `this repository`; Keeper and reference clones are read-only.
- Preserve HEAD `7b7f70f` and the accepted dirty C1-C4 D2B tree. No stage,
  commit, reset, restore, clean, package or publish.

## Inputs

Read all AGENTS files, the accepted C4 design, Task 8 in the C4 implementation
plan, D2A/D2BR2 reports, and existing governed read/write runtime-host bridges.

## Goal

Implement runtime-host coordination for `process.run@1` through the accepted
`GovernedProcessToolGateway`. Provider proposals receive host-owned project,
conversation, run and call identity. Keep process maps separate from governed
read/write maps. One pending process action is scoped to its owning run and
conversation; foreign conversations/projects cannot list, decide or replay it.

A cancelled turn may deliver at most one validated terminal process receipt
before the host-forced run terminal event. Restart interrupts non-terminal
authority. Busy/replay never creates new spawn authority. Do not implement
Tauri/TS commands, renderer UI, Git reads, PTY/shell/Git mutation or another
runtime.

## RED -> GREEN evidence

Add tests first for:

- provider proposal identity injection and invalid proposal rejection;
- one pending process action per owning run;
- cross-project/conversation/run list/decide/open-output denial;
- approve/reject/expiry and terminal replay;
- cancellation ordering with at most one validated terminal receipt;
- restart interruption and no re-execution;
- zero residual host worker/slot/process authority after terminal completion.

Run:

- `cargo test -p keeper-runtime-host --test governed_process_bridge -- --nocapture`
- `cargo test -p keeper-runtime-host --tests`
- `cargo check --workspace --all-targets`
- `cargo clippy -p keeper-runtime-host --all-targets -- -D warnings`
- `cargo fmt --all --check`
- `git diff --check -- crates/runtime-host`

## Exclusive write allowlist

- `crates/runtime-host/Cargo.toml`
- `crates/runtime-host/src/host/process.rs`
- `crates/runtime-host/src/host/mod.rs`
- `crates/runtime-host/src/lib.rs`
- `crates/runtime-host/tests/governed_process_bridge.rs`
- `docs/execution/2026-08-04-wave-c4-d3a-grok1-runtime-host-process-report.md`

Do not edit protocol, policy, storage, process/tool/Git executors, Tauri,
renderer, scripts, root manifests or lock. Stop and report if an out-of-scope
change is required.

## Completion

Report exact changed files, event-order proof, RED/GREEN counts, staging/HEAD,
residual processes and risks. Post one Keeper completion message and emit
`COMPLETION: complete` or a blocker. Controller alone accepts.
