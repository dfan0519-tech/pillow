# C4 D3AR Grok repair report — process authority, events and real output

Date: 2026-08-04
Worker: grok-worker (Keeper session `<keeper-session>`)
Parent: `<keeper-session>`
Task: `c4-d3ar-process-authority-events-output`
Status: **DONE_WITH_CONCERNS** (controller owns acceptance)

## Goal delivered

Repair of rejected D3A Task 8 process authority slice:

1. **Removed runtime-host storage side channel** — no `SqliteExecProposalRepository` open/list/transition/recover; no gateway-pointer/database-path registry. Host uses only `GovernedProcessToolGateway` fixed ops.
2. **Host-private approval grants** — raw grant is memory-only inside live state; public view exposes `decision_available: bool` only; public `decide_process_proposal` accepts conversation/proposal + approve-once/reject only (no grant argument/return).
3. **Actual bounded stdout/stderr** — supervised runner produces stream-tagged chunks with sequence, lossy UTF-8, 256 KiB + 4,000-line caps, control neutralization; gateway returns memory-only `LiveProcessOutput` on fresh spawn; SQLite keeps digests/metadata only; restart has no live transcript.
4. **Four fixed process runtime events** — `process_proposal_requested`, `process_proposal_decided`, `process_started`, `process_terminal_outcome` with deny-unknown-fields payloads; host receipt carries typed payloads in durable order; reject/expire/cancel/interrupt omit start; busy/replay never second start/spawn.
5. **Production test authority gated** — `force_live_process_expiry_for_tests` is `cfg(any(test, feature = "test-support"))` including re-exports and host methods.

## Exact changed files (allowlist only)

| Path | Action |
| --- | --- |
| `crates/agent-protocol/src/types.rs` | Process event kinds + payloads + `ProcessRiskClass` |
| `crates/agent-protocol/src/validate.rs` | Validate four process kinds |
| `crates/agent-protocol/src/lib.rs` | Re-exports + sample payloads + kind count 21 |
| `crates/agent-protocol/tests/process_event_contract.rs` | **New** — schema/order/privacy contract |
| `crates/process-executor/src/lib.rs` | Chunk/observer re-exports; `run_with_observer` |
| `crates/process-executor/src/output.rs` | Chunks, line cap, neutralization, observer |
| `crates/process-executor/tests/supervised_process.rs` | Real text + observer tests |
| `crates/tool-executor/src/process.rs` | Owner-bound get/list/expire/cancel/interrupt/recover; reserve terminal replay; `GovernedProcessExecution` + live output |
| `crates/tool-executor/src/lib.rs` | Public re-exports |
| `crates/tool-executor/tests/governed_process.rs` | Live output + owner-bound + reserve replay |
| `crates/runtime-host/src/host/process.rs` | Full rewrite: gateway-only, private grants, events, output |
| `crates/runtime-host/src/host/mod.rs` | No path registry; grant-free decide; cfg-gated force-expiry |
| `crates/runtime-host/src/lib.rs` | cfg-gated force-expiry re-export |
| `crates/runtime-host/tests/governed_process_bridge.rs` | RED→GREEN authority/events/output suite |
| `docs/execution/2026-08-04-wave-c4-d3ar-grok-process-authority-events-output-report.md` | This report |

`crates/runtime-host/Cargo.toml` unchanged (test-support already present).

HEAD preserved: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`. Staging empty. No stage/commit/reset/restore/clean/package/publish.

## Public API changes and why no second authority

### Gateway (`GovernedProcessToolGateway`)

| Method | Role |
| --- | --- |
| `get_for_owner(capability, conversation_id, proposal_id)` | Capability+conversation read; foreign fails closed |
| `list_for_conversation(capability, conversation_id)` | Capability-filtered list |
| `decide_for_owner(...)` | Capability-checked decide |
| `expire_proposal` / `cancel_proposal` / `interrupt_proposal` | Fixed owned transitions |
| `recover_interrupted(now)` | Fixed startup recovery (no path exposure) |
| `execute` → `GovernedProcessExecution` | Outcome + optional memory-only `LiveProcessOutput` |
| `reserve` | Terminal identity replay via reservation key + fingerprint (no pre-list) |

Repository, generic transition, mutation trait, raw connection, and database path are **not** returned.

### Runtime-host

| Symbol | Change |
| --- | --- |
| `LiveProcessProposalView.decision_available` | Replaces `approval_grant` |
| `decide_process_proposal(conversation, proposal, decision)` | No grant argument |
| `recover_process_proposals` | Uses gateway only |
| `GovernedProcessReceipt` | Adds typed process event payloads |
| `force_live_process_expiry_for_tests` | cfg(test) / `test-support` only |
| Removed | `register_process_gateway_path`, ledger/path maps, host `SqliteExecProposalRepository` use |

**No second authority:** product process coordination mutates durable state only through `GovernedProcessToolGateway`. Host live maps hold grants/output only while the process is live in this process.

## Output / event ordering and privacy proof

### Output

- Fresh `execute` returns real runner-captured text as stream-tagged, sequenced, control-neutralized chunks (`LiveProcessOutput`).
- Combined caps: 256 KiB (`PROCESS_OUTPUT_BYTE_CAP`) and 4,000 display lines (`PROCESS_DISPLAY_LINE_CAP`).
- SQLite `result_json` digests/metadata only (proven in `fresh_execute_returns_real_bounded_live_output`).
- Terminal replay / restart: `live_output` is `None`; open-output is `NotLive`.

### Events (caller-emits pattern)

Order on approve path: **reserve → requested → decided → started → terminal**.
Reject/expire/cancel/interrupt: **no `process_started`**.
Busy/replay: no second start or spawn.

Payloads reject unknown fields and secret/native/output keys (`process_event_contract`).

Grant never appears in views, Debug, events, or SQLite.

## TDD / gates

### Commands and results

| Command | Result |
| --- | --- |
| `cargo test -p keeper-agent-protocol --all-targets` | **PASS** (incl. `process_event_contract` 4/4) |
| `cargo test -p keeper-process-executor --all-targets` | **PASS** (unit 4 + supervised 12) |
| `cargo test -p keeper-tool-executor --all-targets` | **PASS** (`governed_process` 27/27; unit 17; others green) |
| `cargo test -p keeper-runtime-host --all-targets` | **PASS** (`governed_process_bridge` 13/13 ordinary; full suite green) |
| `cargo test -p keeper-runtime-host --features test-support --test governed_process_bridge` | **PASS** 14/14 (incl. expiry) |
| `cargo check -p keeper-agent-protocol -p keeper-process-executor -p keeper-tool-executor -p keeper-runtime-host --all-targets` | **PASS** |
| `cargo clippy -p keeper-process-executor -p keeper-tool-executor -p keeper-runtime-host --all-targets -- -D warnings` | **PASS** |
| `cargo clippy -p keeper-agent-protocol --lib -- -D warnings` | **PASS** |
| `cargo clippy -p keeper-agent-protocol --all-targets -- -D warnings` | **FAIL** — pre-existing `clippy::too_many_arguments` in non-allowlisted `tests/tool_contract.rs:882` (`process_run_output_json`, 9 args). Outside exact write allowlist; not introduced by this repair. |
| `cargo fmt --all --check` | **PASS** |
| `git diff --check -- <allowlist>` | **PASS** (CRLF warnings only) |
| `git diff --cached --name-only` | **empty** |
| `git rev-parse HEAD` | `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b` |
| `cargo check --workspace --all-targets` | Expected residual failure in concurrent Codex2 `pillow-desktop` / missing symbols outside allowlist (not re-verified end-to-end this run after package gates; package-scoped check green). |

### Evidence map (brief requirements)

1. No host repository/path registry — `runtime_host_process_product_code_has_no_storage_side_channel`
2. Foreign project/conversation denied — gateway `owner_bound_*` + host cross-scope test
3. Grant private + approve-once once — bridge + gateway one-shot tests
4. Real stdout/stderr + caps/neutralization — process-executor unit + supervised + gateway live output
5. No transcript after restart — restart bridge test + replay `live_output: None`
6. Event schemas/order/privacy — `process_event_contract` + bridge approve receipt validation
7. Reject/expire no start; replay/busy no second start — bridge + gateway
8. Cancel/interrupt/restart zero residual authority — bridge tests
9. Force-expiry not ordinary import — source cfg gate + `#[cfg(feature = "test-support")]` expiry test
10. Pre-existing D2A/D2B focused tests green — process-executor + tool-executor suites

## HEAD / staging

- HEAD: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
- Staging: empty
- Unrelated dirty tree preserved

## Residual processes

No worker-owned residual process authority after tests. Ambient IDE/node may exist; no PID/name cleanup performed.

## Residual risks (controller)

1. **Pre-existing clippy** in `agent-protocol/tests/tool_contract.rs` blocks full `-p keeper-agent-protocol --all-targets -D warnings`; lib + this package’s other crates are clean. Outside allowlist.
2. **SQLite `run_events.kind` CHECK** still lacks process event kind names (storage not in allowlist). Protocol validates events; durable run_events insert of process kinds would need a later storage migration.
3. **kernel-session** still has a separate `RuntimeEventKind` without process variants (outside allowlist).
4. Workspace-wide check may still fail on concurrent Codex2 desktop RED anchor.
5. Live open-output during/after approve is racy once the waiter consumes the live entry; chunks are retained on the receipt/gateway path for fresh spawn.
6. Not controller source-reviewed; acceptance remains controller-owned.

## Completion

`COMPLETION: complete` is **not** claimed solely because full `clippy -p keeper-agent-protocol --all-targets -D warnings` remains blocked by a pre-existing non-allowlisted lint. All functional repair requirements and package-scoped product tests are green.

**DONE_WITH_CONCERNS**
