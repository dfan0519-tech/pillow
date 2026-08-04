# C4-D2BR2 Grok2 development report — sealed process authority repair

Date: 2026-08-03

Worker: real Grok CLI slot `grok2` (`.grok2`, Grok 4.5 high)
Worker session: `<keeper-session>`
Controller parent session: `<keeper-session>`
Task: `c4-d2br2-sealed-process-authority`
Status: **DONE** (controller owns acceptance)

## Why D2BR1 was rejected (addressed)

Controller source review found two remaining production bypasses after D2BR1:

1. `GovernedProcessToolGateway::store()` publicly returned
   `&SqliteExecProposalRepository`, so a production caller could
   reserve/decide/claim directly and bypass the gateway capability/identity
   contract.
2. `SealedProcessSpec::with_timeout()` was public, mutated the sealed fixed
   60-second timeout after seal, was not fingerprint-bound, and remained
   available in production builds despite being described as a test seam.

## Repairs delivered

| # | Repair | Evidence |
| --- | --- | --- |
| 1 | Removed production public `store()`. Mutable ledger access is `#[cfg(test)] fn store` only. | `public_api_has_no_native_getters_*` asserts `!pub fn store(`; unit claim matrix uses private seam |
| 2 | Added narrow read-only `get_proposal(proposal_id) -> Option<StoredExecProposal>` (metadata-only DTO; no reserve/decide/claim). | `read_only_proposal_inspection_cannot_mutate_ledger`; privacy/rejection tests use `get_proposal` |
| 3 | Removed production public `with_timeout`. Private `#[cfg(test)] fn with_timeout` only for accelerated timeout unit support. | Source API lock; `seal_always_binds_fixed_process_timeout`; `accelerated_timeout_unit_seam_*` |
| 4 | Every production seal retains `PROCESS_TIMEOUT` (60s) through reserved sealed material. | `production_seal_always_uses_fixed_sixty_second_timeout`; unit seal timeout assertion |
| 5 | Cancellation uses a normal long-running child under fixed 60s timeout (pre-spawn cancel); no timeout mutator. | `success_nonzero_timeout_cancel_and_receipt_replay` |
| 6 | Busy/expiry claim matrix: integration uses storage crate ledger API (not `gateway.store()`); unit uses private store seam. Gateway execute still returns Busy / Expired without spawn. | Busy + expiry integration tests + unit claim matrix |
| 7 | Preserved D2BR1: argument contract at seal, capability-bound reserve, no native getters, no public runner injection. | Existing argv/capability/native/runner tests remain green |

## Final public API surface (process gateway)

| Symbol | Role |
| --- | --- |
| `GovernedProcessToolGateway::open` | Production construction (always `SupervisedProcessRunner`) |
| `GovernedProcessToolGateway::seal` | Argument contract + catalog + native resolve (memory-only); timeout = `PROCESS_TIMEOUT` |
| `GovernedProcessToolGateway::reserve` | Capability-bound digests-only reservation |
| `GovernedProcessToolGateway::decide` | Approve-once / reject via ledger |
| `GovernedProcessToolGateway::execute` | One-shot claim → supervised run → typed receipt |
| `GovernedProcessToolGateway::get_proposal` | **Read-only** metadata DTO inspection (no mutation) |
| `SealedProcessSpec` | Memory-only seal; public: program/argv/portable_cwd/digests/**timeout getter**/output_byte_cap; **no** native getters; **no** timeout mutator |
| `PreparedExecProposal` | Durable metadata + sealed material |
| `GovernedProcessOutcome` / `GovernedProcessError` | Outcomes + fail-closed errors |
| `PROCESS_CATALOG`, `PROCESS_TIMEOUT` (60s), `PROCESS_OUTPUT_BYTE_CAP` | Host constants |
| `PROCESS_RUN_ARGV_COUNT_CAP` (64), `PROCESS_RUN_ARGV_BYTE_CAP` (32768) | Re-exported protocol bounds |
| `validate_process_run_arguments` / `validate_program_catalog` | Explicit validators |
| `build_clean_environment` / `CLEANUP_UNPROVEN_PUBLIC_ERROR` | Env + public cleanup token |

**Not public in normal builds:** `ProcessRunner`, `CountingProcessRunner`,
`CleanupUnprovenRunner`, `open_with_runner`, `reserve_with_input`,
`store()`, `with_timeout()`, `executable()`, `native_cwd()`.

No other public method mutates timeout, output cap, executable, native cwd,
runner, reservation identity, or claim state outside governed
`seal` / `reserve` / `decide` / `execute`.

## Exact files (exclusive allowlist)

| Path | Action |
| --- | --- |
| `crates/tool-executor/src/process.rs` | **Modified** — sealed authority repair |
| `crates/tool-executor/src/lib.rs` | **Unchanged** this slice (exports already sufficient) |
| `crates/tool-executor/tests/governed_process.rs` | **Modified** — RED/GREEN authority suite |
| `docs/execution/2026-08-03-wave-c4-d2br2-grok2-sealed-authority-repair-report.md` | **Created** (this report) |

No manifest/lock or other crate/app/script edits.

HEAD preserved: `7b7f70f`. No stage, commit, reset, restore, clean, package or
publish. Pre-existing C1–C4 dirty tree outside allowlist left untouched.

## TDD gates

### RED (pre-repair, documented)

Before this repair, production consumers **could**:

- call `gateway.store()` and then `reserve` / `decide` / `claim_run` /
  `complete_run` outside the gateway contract;
- call `sealed.with_timeout(...)` after seal and change the fixed 60s timeout
  without fingerprint binding.

### GREEN

| Command | Result |
| --- | --- |
| `cargo test -p keeper-tool-executor --test governed_process -- --nocapture` | **24/24** passed |
| `cargo test -p keeper-tool-executor --tests` | **84** passed (lib 17 incl. process unit; governed_gateway 4; governed_process 24; governed_write 17; read_executor 10; write_diff 12) |
| `cargo check --workspace --all-targets` | exit 0 |
| `cargo clippy -p keeper-tool-executor --all-targets -- -D warnings` | exit 0 |
| `cargo fmt --all --check` | exit 0 |
| `git diff --check -- crates/tool-executor` | exit 0 |

New authority-focused coverage:

- `public_api_has_no_native_getters_or_arbitrary_runner_injection` — no `pub fn store` / `pub fn with_timeout`
- `production_seal_always_uses_fixed_sixty_second_timeout`
- `read_only_proposal_inspection_cannot_mutate_ledger`
- unit: `seal_always_binds_fixed_process_timeout`
- unit: `accelerated_timeout_unit_seam_marks_timed_out_without_production_mutator`
- unit: `claim_matrix_busy_without_spawn_via_private_store_seam`

## Staging state

- Allowlist deliverables: unstaged only (`?? crates/tool-executor/`, report untracked).
- `git diff --cached --name-only`: empty for this worker.
- HEAD: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`.

## Residual processes

Ambient IDE/`node` processes exist on the host. No residual
`cargo`/`rustc`/`pillow`/`fake_acp` children left that this worker owns after
test completion. Tests rely on Job Object cleanup inside
`SupervisedProcessRunner`; this worker did not kill by numeric PID or process
name.

## Residual risks (controller)

1. Integration Busy/expiry setup opens `SqliteExecProposalRepository` on the
   same DB path for ledger claim injection. That is the storage crate’s public
   API, not a gateway production bypass; product callers still cannot obtain a
   mutation-capable repository from the gateway.
2. Accelerated timeout is unit-only (`cfg(test)`). Full 60s live timeout is not
   exercised in integration (by design).
3. Runtime-host product wiring for process.run remains separate; this crate is
   seal/apply authority only.
4. Not controller source-reviewed; acceptance remains controller-owned.

## Completion

`COMPLETION: complete`
