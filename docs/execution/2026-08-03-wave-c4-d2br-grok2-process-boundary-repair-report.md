# C4-D2BR Grok2 development report — process gateway boundary repair

Date: 2026-08-03

Worker: real Grok CLI slot `grok2`
Worker session: `<keeper-session>`
Controller parent session: `<keeper-session>`
Task: `c4-d2br-process-boundary-repair`
Status: **DONE** (controller owns acceptance)

## Why D2B was rejected (addressed)

Controller source review found public-gateway bypasses that green focused tests
did not catch:

1. `seal` did not enforce the full `process.run@1` argument contract.
2. Public `reserve_with_input` accepted caller-selected project/identity.
3. `SealedProcessSpec` exposed public native executable/cwd getters.
4. Public `open_with_runner` + exported doubles allowed arbitrary production
   runner injection.

## Repairs delivered

| # | Repair | Evidence |
| --- | --- | --- |
| 1 | `seal` always runs `validate_process_run_arguments` before catalog resolve: program grammar `^[a-z][a-z0-9_-]{0,31}$`, ≤64 argv, ≤32 KiB combined program+argv, no NUL/control, portable cwd grammar. Single entry path — no raw skip. | `argv_count_boundary_*`, `combined_program_argv_byte_boundary_*`, `nul_and_control_*` (integration + unit) |
| 2 | Public reservation is only `reserve(capability, conversation, run, call, sealed)`. Project/conversation/run/call identities are capability-derived. `reserve_with_input` removed from the public surface. Execute with foreign `ProjectCapability` → `CapabilityMismatch`. | `production_reserve_always_binds_*`, `reservation_with_mismatched_project_capability_is_denied`, source API lock |
| 3 | Removed `pub fn executable` / `pub fn native_cwd`. Native material remains private fields; Debug redacts; safe `argv()` display retained. | `public_api_has_no_native_getters_*`, `sealed_argv_and_native_paths_never_enter_sqlite` |
| 4 | Production `open` always installs `SupervisedProcessRunner`. `open_with_runner`, counting/cleanup doubles, and `ProcessRunner` trait are `cfg(test)`-only (not crate-root re-exported). | `public_api_has_no_native_getters_*`, unit `counting_runner_*` / `cleanup_unproven_*` |
| 5 | Preserved: drift/expiry after claim → durable terminal failure, Busy never spawns, clean env, cancellation receipt, cleanup fail-closed. | Existing lifecycle + Busy tests + unit cleanup path |

## Final public API surface (process gateway)

| Symbol | Role |
| --- | --- |
| `GovernedProcessToolGateway::open` | Production construction (always `SupervisedProcessRunner`) |
| `GovernedProcessToolGateway::seal` | Argument contract + catalog + native resolve (memory-only) |
| `GovernedProcessToolGateway::reserve` | Capability-bound digests-only reservation |
| `GovernedProcessToolGateway::decide` | Approve-once / reject via ledger |
| `GovernedProcessToolGateway::execute` | One-shot claim → supervised run → typed receipt |
| `GovernedProcessToolGateway::store` | Ledger access |
| `SealedProcessSpec` | Memory-only seal; public: program/argv/portable_cwd/digests/timeout; **no** native getters |
| `PreparedExecProposal` | Durable metadata + sealed material |
| `GovernedProcessOutcome` / `GovernedProcessError` | Outcomes + fail-closed errors (`InvalidArguments`, `CapabilityMismatch`, …) |
| `PROCESS_CATALOG`, `PROCESS_TIMEOUT`, `PROCESS_OUTPUT_BYTE_CAP` | Host constants |
| `PROCESS_RUN_ARGV_COUNT_CAP` (64), `PROCESS_RUN_ARGV_BYTE_CAP` (32768) | Re-exported protocol bounds |
| `validate_process_run_arguments` / `validate_program_catalog` | Explicit validators |
| `build_clean_environment` / `CLEANUP_UNPROVEN_PUBLIC_ERROR` | Env + public cleanup token |

**Not public in normal builds:** `ProcessRunner`, `CountingProcessRunner`,
`CleanupUnprovenRunner`, `open_with_runner`, `reserve_with_input`,
`executable()`, `native_cwd()`.

## Capability-binding proof

- `reserve` always sets `project_id = capability.project_id()` and builds
  `reservation_key = ep:{project}:{conversation}:{run}:{call}`.
- Empty capability project id → `InvalidProjectRoot`.
- `execute` rejects `current.project_id != capability.project_id()` as
  `CapabilityMismatch` before claim/spawn.
- No production method accepts an external `ExecProposalReserveInput`.

## Exact files (exclusive allowlist)

| Path | Action |
| --- | --- |
| `crates/tool-executor/src/process.rs` | **Modified** — boundary repair |
| `crates/tool-executor/src/lib.rs` | **Modified** — public exports |
| `crates/tool-executor/tests/governed_process.rs` | **Modified** — RED/GREEN boundary suite |
| `docs/execution/2026-08-03-wave-c4-d2br-grok2-process-boundary-repair-report.md` | **Created** (this report) |

`Cargo.toml` unchanged (no feature required; test doubles stay under `cfg(test)`).

HEAD preserved: `7b7f70f`. No stage, commit, reset, restore, clean, package or
publish. Pre-existing C1–C4 dirty tree outside allowlist left untouched.

## TDD gates

### RED (pre-repair, documented)

Before this repair, D2B green tests did **not** fail on:

- 65 argv entries (accepted by seal)
- >32 KiB combined program/argv text
- control characters in argv at seal
- caller-selected identity via public `reserve_with_input`
- public native getters / `open_with_runner`

New tests encode those denials at the production boundary.

### GREEN

| Command | Result |
| --- | --- |
| `cargo test -p keeper-tool-executor --test governed_process -- --nocapture` | **22/22** passed |
| `cargo test -p keeper-tool-executor --tests` | **79** passed (lib 14 incl. process unit; governed_gateway 4; governed_process 22; governed_write 17; read_executor 10; write_diff 12) |
| `cargo check -p keeper-tool-executor --all-targets` | exit 0 |
| `cargo check --workspace --all-targets --exclude keeper-git-executor` | exit 0 |
| `cargo clippy -p keeper-tool-executor --all-targets -- -D warnings` | exit 0 |
| `cargo fmt -p keeper-tool-executor -- --check` | exit 0 |
| `git diff --check -- crates/tool-executor` | exit 0 |
| `cargo check --workspace --all-targets` | **blocked** by concurrent out-of-allowlist `keeper-git-executor` (`git_read_integration.rs` type error). Not caused by this worker. |
| `cargo fmt --all --check` | **blocked** by concurrent dirty `keeper-git-executor` formatting (outside allowlist). Package-scoped fmt is green. |

## Staging state

- Allowlist deliverables: unstaged only (`?? crates/tool-executor/`, report untracked).
- `git diff --cached --name-only`: empty for this worker.
- HEAD: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`.

## Residual processes

Ambient IDE/`node` processes may exist on the host. No residual
`cargo`/`rustc`/`pillow`/`fake_acp` children left from this worker's test runs.
Tests rely on Job Object cleanup inside `SupervisedProcessRunner`; this worker
did not kill by numeric PID or process name.

## Residual risks (controller)

1. Full workspace `cargo check --all-targets` / `cargo fmt --all --check` remain
   red solely due to parallel D2BR git-containment work outside this allowlist.
2. Host still needs catalog tools (`node` required for live suite paths).
3. Runtime-host product wiring (Task 8) still required; this crate remains the
   seal/apply authority only.
4. Not controller source-reviewed; acceptance remains controller-owned.

## Completion

`COMPLETION: complete`
