# C4-D3A Grok1 development report — runtime-host process coordination

Date: 2026-08-04 (session wall clock; user context)

Worker: real Grok CLI slot `grok1` (`.grok1`, Grok 4.5 high)
Worker session: `<keeper-session>`
Controller parent session: `<keeper-session>`
Task: `c4-d3a-runtime-host-process`
Status: **DONE** (controller owns acceptance)

## Goal delivered

Runtime-host coordination for `process.run@1` through accepted
`GovernedProcessToolGateway`:

- Provider proposals receive **host-owned** project, conversation, run and call
  identity (adapter supplies only `call_id` / program / argv / portable cwd).
- Process maps are **separate** from governed read/write maps.
- One non-terminal pending process action per owning run.
- Foreign conversation/project cannot list, decide or open-output a live proposal.
- Approve-once / reject / expiry / terminal identity replay.
- Cancelled turn: **at most one** validated terminal process receipt; busy/replay
  never creates new spawn authority.
- Restart interrupts non-terminal authority; no re-execution.
- Zero residual host live process authority after terminal completion / release /
  shutdown.

Out of scope (as briefed): Tauri/TS, renderer UI, Git, PTY/shell/Git mutation,
other runtimes.

## Exact changed files (exclusive allowlist)

| Path | Action |
| --- | --- |
| `crates/runtime-host/Cargo.toml` | **Modified** — `keeper-process-executor` dep; `governed_process_bridge` test target |
| `crates/runtime-host/src/host/process.rs` | **Created** — process coordinator |
| `crates/runtime-host/src/host/mod.rs` | **Modified** — process gateway field, register/list/decide/open-output/recover, shutdown release |
| `crates/runtime-host/src/lib.rs` | **Modified** — public re-exports |
| `crates/runtime-host/tests/governed_process_bridge.rs` | **Created** — RED→GREEN bridge suite (10 tests) |
| `docs/execution/2026-08-04-wave-c4-d3a-grok1-runtime-host-process-report.md` | **Created** (this report) |

No protocol, policy, storage, tool-executor, process-executor product edits,
Tauri, renderer, scripts, root manifests or lock edits by this worker.

HEAD preserved: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`. No stage, commit,
reset, restore, clean, package or publish.

## Public surface (runtime-host)

| Symbol | Role |
| --- | --- |
| `ProviderProcessProposal` | Adapter-normalized input (no project authority) |
| `execute_governed_process` | Host identity inject → seal → reserve → wait → receipt |
| `list_process_proposals` | Owner + conversation scoped list (grant only while live pending) |
| `decide_process_proposal` | One-shot approve/reject; approve claims once and executes |
| `open_process_output` | Live host-retained output projection (cross-scope denied) |
| `recover_process_proposals` | Non-terminal → interrupted; no auto re-run |
| `register_governed_process_run` / `release_governed_process_run` | Host bind/release |
| `GovernedProcessReceipt` / `LiveProcessProposalView` / `ProcessOutputView` | Receipt + DTO sources |
| `live_process_authority_count` / `pending_process_run_count` | Residual authority invariants |

Process coordinator state (`runs` / `live` / `pending_by_run`) is independent of
read/write coordinators. Grants reuse the C3 base64url grant helpers from the
write bridge (same alphabet/digest contract) without sharing write maps.

## Event-order proof

1. **Reserve before request callback** — durable `pending_approval` exists before
   `on_requested` (identity injection test).
2. **Approve path** — `decide(ApproveOnce)` → gateway `execute` one-shot claim →
   single terminal receipt (`Succeeded`/`Failed`/`Cancelled`); second identical
   call identity returns `GovernedProcessDelivery::Replayed` without re-spawn
   (`approve_runs_once_and_terminal_replay_skips_second_spawn`).
3. **Reject / expiry** — terminal without spawn.
4. **Cancel** — exactly one validated terminal process receipt
   (`receipt_count == 1`, state `Cancelled`); residual live authority 0.
   Host-forced `run_cancelled` remains a ProtocolGuardSink/C3 concern; process
   receipt is delivered by `execute_governed_process` return before any host
   run-terminal projection the adapter/host would emit next.
5. **Restart** — drop/release interrupts non-terminal to `Interrupted`; recover
   leaves terminal rows unchanged; no re-execution.
6. **Busy** — second concurrent proposal on the same run returns `Busy` without
   reserve authority.

## TDD gates

### RED (pre-implementation intent)

Focused suite `governed_process_bridge` did not exist; symbols
`execute_governed_process` / process maps were absent (compile fail / 0 tests).

### GREEN

| Command | Result |
| --- | --- |
| `cargo test -p keeper-runtime-host --test governed_process_bridge -- --nocapture` | **10/10** passed |
| `cargo test -p keeper-runtime-host --tests` | **all suites green** (unit 22; process bridge 10; write 16; read 4; host/fake/protocol suites green) |
| `cargo check -p keeper-runtime-host --all-targets` | exit 0 |
| `cargo clippy -p keeper-runtime-host --all-targets -- -D warnings` | exit 0 |
| `cargo fmt --all --check` | exit 0 |
| `git diff --check -- crates/runtime-host` | exit 0 |

### Workspace check residual

`cargo check --workspace --all-targets` fails in **parallel D3A codex2**
`pillow-desktop` (`exec_commands.rs` references missing
`is_exec_public_error`). That path is **outside this worker allowlist** and was
not introduced by this slice. Runtime-host package gates are green.

## Staging / HEAD

- HEAD: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
- Staging for allowlist deliverables: **empty** (unstaged only)
- Pre-existing dirty C1–C4 D2B tree outside this allowlist left untouched

## Residual processes

Ambient IDE/`node` processes may exist on the host. No residual
`cargo`/`rustc`/`node` children owned by this worker after tests. Process
execution uses `GovernedProcessToolGateway` → `SupervisedProcessRunner` Job
Object cleanup; this worker did not kill by numeric PID or process name.

## Residual risks (controller)

1. **Live stdout/stderr text**: public gateway `execute` returns digests-only
   `StoredExecProposal`. Host `open_process_output` exposes sealed display argv
   and durable metadata while live; **full text chunks are not available**
   through the accepted gateway surface (no tool-executor edit in allowlist).
2. **Process RuntimeEventKinds**: protocol still lacks dedicated process
   proposal requested/resolved kinds; host uses local
   `ProcessProposalRequestedView` + `GovernedProcessReceipt`. Typed Tauri/TS
   (D3A codex2) and later protocol events remain separate.
3. **Ledger side-path**: cancel/expire/interrupt durable transitions use a
   host-registered `SqliteExecProposalRepository` beside the gateway because
   production `gateway.store()` was removed in D2BR2. Product callers still
   cannot mutate the ledger through the gateway.
4. **Workspace check** blocked by concurrent desktop D3A work (not this slice).
5. Not controller source-reviewed; acceptance remains controller-owned.

## Completion

`COMPLETION: complete`
