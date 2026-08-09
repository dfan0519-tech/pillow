# Pillow C4 D2B - governed process gateway development brief

## Worker identity and session

- Worker: real Grok CLI slot `grok2`, model Grok 4.5, high reasoning.
- Keeper boot: `project=pillow`, `task=c4-d2b-governed-process-gateway`,
  `agentName=grok2`, `agentRole=worker`,
  `<local-parent-session>`.
- Repository: `this repository` only. Keeper and references are read-only.
- Preserve HEAD `7b7f70f` and the accepted dirty C1-C3/D1/D2A tree. No stage,
  commit, reset, restore, clean, package or publish.

## Inputs to read completely

1. All applicable AGENTS files.
2. `docs/superpowers/specs/2026-08-02-c4-governed-process-git-design.md`.
3. Task 7 in `docs/superpowers/plans/2026-08-02-c4-governed-process-git-implementation.md`.
4. D1 protocol/policy and D2A process/ledger code and reports, including both
   D2AR repair reports.
5. Existing governed read/write gateways as patterns, without weakening C3.

## Goal and locked boundary

Implement the in-process `seal -> decide -> claim -> execute -> receipt`
gateway for `process.run@1`. Provider input contains only fixed program label,
structured argv and optional portable cwd. Host resolves the executable and
native cwd, builds the clean environment, seals native material in memory and
persists only metadata/digests through the accepted execution ledger.

The public process catalog must deny `git`, shells and script shims. There is
no direct `Command::spawn` in `tool-executor`; execution must go through
`SupervisedProcessRunner`. A claim result other than the first `Claimed` never
spawns. Cancelled execution must complete with the accepted typed cancelled
receipt. Cleanup uncertainty fails closed with a fixed public error.

## Required RED -> GREEN evidence

Add tests before implementation for:

- exact allowlisted program-label resolution and denial of unknown labels;
- denial of Git, PowerShell, cmd, shell and script-shim routes;
- argv/cwd fingerprint binding, executable/cwd drift and expiry;
- approval/rejection and exactly one spawn authority;
- same-owner, other-owner and expired Running claims returning Busy without
  spawn;
- clean environment and sensitive-name exclusion;
- success, nonzero exit, timeout, cancellation and bounded receipt replay;
- unproven cleanup returning a typed failure, never success.

Run:

- `cargo test -p keeper-tool-executor --test governed_process -- --nocapture`
- `cargo test -p keeper-tool-executor --tests`
- `cargo check --workspace --all-targets`
- `cargo clippy -p keeper-tool-executor --all-targets -- -D warnings`
- `cargo fmt --all --check`
- `git diff --check -- crates/tool-executor`

## Exclusive write allowlist

- `crates/tool-executor/Cargo.toml`
- `crates/tool-executor/src/process.rs`
- `crates/tool-executor/src/lib.rs`
- `crates/tool-executor/tests/governed_process.rs`
- `docs/execution/2026-08-03-wave-c4-d2b-grok2-governed-process-gateway-report.md`

Do not modify `Cargo.toml`, `Cargo.lock`, protocol, policy, process-executor,
storage, runtime-host, Tauri, renderer, Git executor or scripts. Stop and report
if compilation genuinely requires an out-of-allowlist change.

## Completion contract

Report exact changed files, RED/GREEN outputs and counts, denial matrix,
spawn-authority proof, privacy evidence, staging state, residual processes and
risks. Post one Keeper completion message to the parent session and emit
`COMPLETION: complete` or a precise blocker. The controller owns acceptance.
