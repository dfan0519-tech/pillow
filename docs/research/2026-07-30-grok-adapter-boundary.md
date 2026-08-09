# Grok ACP Adapter Boundary Research

Date: 2026-07-30  
Status: Read-only architecture map for Slice B2  
Worktree: `wave-b2-boundary`  
Scope: Map accepted B1 Rust interfaces to a **minimal** Grok ACP adapter without leaking provider state upward or duplicating `RuntimeHost` responsibilities.

## 0. Non-goals and constraints

- **Read-only analysis.** This document does not change product code.
- **Slice B1 is fixed.** Provider-neutral protocol types, host lifecycle, SQLite authority, and conversation service ownership stay as accepted.
- **Slice B2 is real Grok ACP only.** `FakeRuntimeAdapter` is test evidence and cannot satisfy B2 (design + B1 acceptance).
- **Legacy is reference, not copy.** Port behavior through typed contracts; do not copy `legacy Keeper (out of tree)` files wholesale.
- **Every new source file must stay under 500 lines.**
- **Secrets never enter events, logs, SQLite payloads, or renderer state.**
- **Domain state remains in SQLite; provider wire state remains inside the adapter crate.**

---

## 1. Current trait and ownership map

### 1.1 Layer stack (accepted B1)

```text
React UI
  -> ConversationGateway (typed Tauri invoke / listen)
  -> conversation_commands (Tauri)
  -> ConversationService          [apps/desktop src-tauri]
       |-- SqliteConversationRepository   [storage-sqlite / kernel-session]
       |-- RuntimeHost                    [runtime-host]
       |      |-- ProtocolGuardSink / BoundedDelivery / workers
       |      \-- Arc<dyn AgentRuntimeAdapter>
       |             \-- FakeRuntimeAdapter (B1 only)
       \-- RuntimeEventEmitter (renderer after durable commit)

Authoritative per-event order (ConversationService pump):
  validate -> persist (+ status) -> commit -> emit
```

### 1.2 Trait ownership

| Trait / surface | Crate | Owner role |
| --- | --- | --- |
| `AgentRuntimeAdapter` | `keeper-agent-protocol` | Provider-neutral adapter contract |
| `EventSink` | `keeper-agent-protocol` | Ordered event ingress for one turn |
| `ConversationRepository` | `keeper-session` | Durable conversations / runs / events |
| `RuntimeEventEmitter` | desktop `conversation_events` | Post-commit renderer fan-out |
| *(no process trait today)* | — | Child process is **not** a host concern |

### 1.3 `AgentRuntimeAdapter` (B1 accepted)

```text
probe() -> RuntimeProbe
start_session(StartSession) -> RuntimeSession
arm_turn(RunId)             // required for spawn_turn cancel race
disarm_turn(RunId)          // spawn failure cleanup
send_turn(SendTurn, &mut dyn EventSink)
cancel_turn(RunId)
shutdown()
```

Design §6.4 also lists `capabilities`, `resume_session`, `stream_events`, and `export_diagnostics`. **B1 intentionally folded streaming into `send_turn` + `EventSink`.** B2 must not re-open a second stream API on the host.

### 1.4 Ownership by component

| Concern | Owner today | Must **not** move into Grok adapter |
| --- | --- | --- |
| Monotonic sequence, run-id match, payload validation, single terminal | `ProtocolGuardSink` (`runtime-host`) | Adapter may assign provisional sequences; host re-validates |
| Concurrent turn slots, worker registry, join/Drop cancel | `RuntimeHost` | No second concurrency pool that races host |
| Bounded event queue / backpressure | `BoundedDelivery` | Adapter blocks on `EventSink::emit` backpressure only |
| SQLite append, run status, timeline, restart `Interrupted` | `ConversationService` + repository | Adapter never opens SQLite |
| Active pump registry, admission vs shutdown | `ConversationService` | Adapter unaware of pumps |
| Renderer DTOs / Tauri events | `conversation_events` | Adapter never emits to UI |
| Provider JSON-RPC, child stdin/stdout, ACP session ids, tool wire shapes | **(missing — B2 crate)** | Host/domain must not parse ACP |

### 1.5 Identifier planes (must stay separate)

| Id | Plane | Notes |
| --- | --- | --- |
| `keeper_session::ConversationId` | Domain / SQLite | Project-scoped conversation |
| `keeper_session::RunId` | Domain / SQLite | Durable run row |
| `keeper_agent_protocol::RunId` | Protocol | Same UUID bytes as domain run for B1 integration |
| `keeper_agent_protocol::RuntimeSessionId` | Protocol | Host-facing session handle; **not** ACP `sessionId` |
| ACP `sessionId` (string) | **Adapter-private** | Must never appear on protocol payloads or DTOs |
| Child PID / job object | **Adapter-private** | Host cancels by `RunId` only |

### 1.6 Public test seams that currently ship as API surface

| Seam | Location | Production default |
| --- | --- | --- |
| `HostConfig.after_worker_registered` | `runtime-host` (`#[doc(hidden)]` but public field) | `None` |
| `PumpRegistrationHooks` / `open_with_config_and_pump_hook` | `conversation_service` | no-ops / not used by app open |
| `FakeRuntimeAdapter` + fixture loaders | `runtime-host` public exports | Desktop always `RuntimeHost::with_fake` in B1 |
| Observable counts (`active_run_count`, `worker_registry_len`, …) | service + host | diagnostics-safe |

See §7 for isolation strategy.

---

## 2. ACP method / update → Keeper command / event mapping

### 2.1 Transport baseline (legacy + ACP v1)

Legacy `grok-acp-client.js` is the behavioral reference:

- **Transport:** newline-delimited JSON-RPC 2.0 over child **stdio**.
- **Launch shape (legacy `buildAcpLaunch`):**  
  `executable args: ["agent", "--no-leader", "stdio"]`  
  `cwd` = project path; env is allow-listed OS vars + isolated `HOME` / `USERPROFILE` / optional `APPDATA` / `LOCALAPPDATA` for account home.  
  **No API keys on the wire message surface** (credentials live in process home / OS store).
- **Client → agent requests:** `initialize`, `session/new`, `session/load`, `session/prompt`.
- **Client → agent notifications:** `session/cancel`, `$/cancel_request`.
- **Agent → client notifications:** `session/update`.
- **Agent → client requests:** `session/request_permission` (legacy auto-answers via tool policy).

B2 ports this **algorithm**, not the JS module graph (no prompt packets, no completion JSON schema, no company harness hooks).

### 2.2 Command mapping (`AgentRuntimeAdapter` ↔ ACP)

| Keeper adapter method | ACP / process action | Notes |
| --- | --- | --- |
| `probe` | Ensure binary resolvable; optional lightweight `initialize` or version check | Return `RuntimeProbe { adapter_name: "grok-acp", version, healthy, detail }` — **no credentials, no account labels** |
| `start_session` | Ensure child alive; `session/new` with `cwd` = `StartSession.working_directory` (required for real Grok); map result to new `RuntimeSessionId` | Store ACP `sessionId` only in adapter map |
| *(optional later)* durable resume | `session/load` when capability `loadSession` and opaque resume token present | **Not required for minimal B2;** see §6 |
| `arm_turn` | Register cancel token for `run_id` **before** host worker starts `send_turn` | Required for `spawn_turn` races; mirror fake adapter |
| `disarm_turn` | Drop armed token if spawn fails | No ACP traffic |
| `send_turn` | Bind `run_id` → session; emit `RunStarted`; `session/prompt` with `prompt: [{ type: "text", text }]`; stream `session/update` into `EventSink`; map `stopReason` to terminal | Blocks until terminal or cancel; cooperative with `cancel_turn` |
| `cancel_turn` | Set cancel flag; write `session/cancel` for ACP session; `$/cancel_request` for in-flight `session/prompt` id | Idempotent if already terminal |
| `shutdown` | Cancel all active turns; settle pending RPCs; stdin end; wait/kill process tree; clear session maps | Host joins workers **after** adapter shutdown (existing host order) |

### 2.3 `session/update` → `RuntimeEventKind`

ACP `params.update.sessionUpdate` values (protocol v1) map as follows. Unlisted / unknown updates are **dropped** (logged at debug) — never forwarded as raw JSON.

| ACP `sessionUpdate` | Keeper `RuntimeEventKind` | Payload construction |
| --- | --- | --- |
| `agent_message_chunk` | `AssistantDelta` | `text` from content block (`type == "text"`) |
| `agent_thought_chunk` | `ReasoningSummaryDelta` | `text` from thought content |
| `plan` | `PlanUpdated` | Map entries → `PlanStep { id, title, status }` (string statuses as agent provides, already neutral) |
| `tool_call` (first sight) | `ToolCallStarted` | `tool_call_id`, `name`/`title`, optional `summary` |
| `tool_call` / `tool_call_update` with content/output | `ToolCallOutput` | `tool_call_id`, `chunk` (sanitized text; strip secrets) |
| `tool_call_update` terminal status | `ToolCallCompleted` | Map status → `completed` / `failed` / `cancelled` |
| *(no first-class subagent update in thin legacy client)* | `SubagentStarted` / `Progress` / `Completed` | **Only** if Grok emits a recognizable structured form; otherwise **omit** in minimal B2 (do not invent) |
| *(file artifacts if present in tool content)* | `ArtifactProduced` | Only when path is workspace-relative and kind is known; else skip |
| *(none)* | `Heartbeat` | Adapter-generated optional tick while prompt open (host/domain already accept kind); not an ACP type |
| `user_message_chunk` | *(none)* | Ignore (Keeper already owns user message in SQLite) |
| `available_commands_update` | *(none)* | Defer (slash commands UI later) |
| `current_mode_update` | *(none)* | Defer (permission mode UI later) |
| `session_info_update` (unstable) | *(none)* | Drop |

### 2.4 Terminal mapping

| Condition | Terminal event |
| --- | --- |
| `session/prompt` result `stopReason` in {`end_turn`, `max_tokens`, …} and not cancelled | `TurnCompleted` (`summary` optional, never raw provider dump) |
| Prompt RPC error / child crash / protocol violation mid-turn | `TurnFailed` `{ code, message }` with **sanitized** message |
| Host or user cancel wins | `RunCancelled` `{ reason }` — host `ProtocolGuardSink` may force this if adapter is slow |
| Cancel after partial stream | Exactly one terminal: prefer `RunCancelled` over late `TurnCompleted` (host cancel arbitration already enforces this on ingress) |

### 2.5 Agent-originated `session/request_permission`

| Legacy | B2 minimal |
| --- | --- |
| Auto-decide via `keeper-acp-tool-policy` | **Do not** port the full policy engine into the adapter |
| | Respond with a **deny / cancel-safe** default for write/network/process classes **or** hold request until a future policy crate exists |
| | Slice C/D will own real approvals; leaking approval UX into B2 is rejected |

Recommendation for B2 gate: **auto-allow read-only tool classes only if the ACP payload marks them as read; otherwise return cancel/reject outcome** so the turn can complete or fail without a hidden auto-write. Record nothing secret. Full Ask-mode approvals remain a later slice.

### 2.6 What must never cross the boundary

Forbidden on `RuntimeEvent.payload` (already enforced by `validate_payload` + `deny_unknown_fields`):

- `api_key`, tokens, `authorization`, `provider`, `acp_session`, raw provider envelopes, account home paths, hook capabilities, lease/slot ids.

Adapter internal logs must redacting-match the same list.

---

## 3. Proposed crate / module tree (every file &lt; 500 lines)

New workspace member only (design name preserved):

```text
crates/adapter-grok-acp/
  Cargo.toml                          # package: keeper-adapter-grok-acp
  src/
    lib.rs                            # re-exports; ≤ 80 lines
    config.rs                         # GrokAcpConfig: executable, env policy, limits
    error.rs                          # map I/O / RPC → RuntimeError (private helpers)
    adapter.rs                        # GrokAcpAdapter: AgentRuntimeAdapter impl
    process/
      mod.rs
      child.rs                        # spawn, stdin write, stdout reader thread, kill tree
      env.rs                          # allow-listed env + isolated home (no secrets in structs Debug)
    rpc/
      mod.rs
      framer.rs                       # newline JSON-RPC read/write, max line bytes
      client.rs                       # request id map, timeouts, notifications dispatch
      methods.rs                      # initialize / session_* typed request builders
    map/
      mod.rs
      session.rs                      # RuntimeSessionId ↔ ACP sessionId (private)
      events.rs                       # session/update → RuntimeEvent construction
      terminal.rs                     # stopReason / errors → terminal events
    turn.rs                           # arm/disarm/active prompt state machine
  tests/
    protocol_fixtures.rs              # JSONL ACP fixture replay against mapper unit tests
    adapter_fake_child.rs             # integration: real adapter + fake child binary
  tests/bin/
    fake_acp_agent.rs                 # stdio ACP child for tests (workspace [[bin]] or test bin)
```

Approximate size budget (hard cap 500 lines each):

| File | Target lines | Responsibility |
| --- | --- | --- |
| `lib.rs` | &lt; 80 | module tree + public exports |
| `config.rs` | &lt; 150 | paths, timeouts, max line/response sizes |
| `adapter.rs` | &lt; 350 | trait impl orchestration only |
| `process/child.rs` | &lt; 350 | Windows-aware process lifecycle |
| `process/env.rs` | &lt; 200 | allow-list env construction |
| `rpc/framer.rs` | &lt; 200 | buffer / line limits |
| `rpc/client.rs` | &lt; 400 | pending map + dispatch |
| `rpc/methods.rs` | &lt; 200 | params structs |
| `map/events.rs` | &lt; 400 | update → event |
| `map/session.rs` | &lt; 150 | id maps |
| `map/terminal.rs` | &lt; 150 | terminals |
| `turn.rs` | &lt; 300 | cancel tokens / busy state |
| `fake_acp_agent.rs` | &lt; 400 | scripted child |

**Not in this crate:** SQLite, `ProtocolGuardSink`, Tauri, React, credential vault UI.

**Workspace `Cargo.toml` change (later implementation):** add `crates/adapter-grok-acp` member; desktop depends on it only at the service composition point.

### 3.1 Optional small protocol extension (prefer avoid)

If process failures need distinct codes, add only neutral `RuntimeError` variants in `agent-protocol` (e.g. `Transport { message }`) — **no ACP types**. Prefer mapping into existing `Internal` / `InvalidEvent` / `Cancelled` for minimal B2.

---

## 4. Exact public types and crate ownership

### 4.1 Remain in `keeper-agent-protocol` (unchanged contract)

Public exports that adapters and host share:

| Type | Role |
| --- | --- |
| `AgentRuntimeAdapter`, `EventSink` | Traits |
| `RuntimeError` | Error vocabulary |
| `RunId`, `RuntimeSessionId` | Protocol ids |
| `RunStatus`, `RuntimeEventKind` | Fixed enums (15 kinds) |
| `RuntimeEvent` | Envelope `{ run_id, sequence, kind, payload, created_at }` |
| Payload structs (`AssistantDeltaPayload`, …) | Kind-specific shapes |
| `StartSession`, `RuntimeSession`, `SendTurn`, `RuntimeProbe` | Commands / results |
| `validate_payload` | Boundary canonicalization |

**Rule:** no ACP, Grok, JSON-RPC, or stdio types in this crate.

### 4.2 Remain in `keeper-runtime-host`

| Type | Role |
| --- | --- |
| `RuntimeHost`, `HostConfig`, `SpawnedTurn` | Concurrency + workers |
| `ProtocolGuardSink`, `BoundedDelivery`, sinks | Ingress policy |
| `FakeRuntimeAdapter`, fixture helpers | **B1 deterministic evidence only** |

Host continues to accept `Arc<dyn AgentRuntimeAdapter>`. No Grok-specific branch inside host.

### 4.3 New public surface of `keeper-adapter-grok-acp`

Minimal public API:

```text
// config.rs
pub struct GrokAcpConfig {
    pub executable: PathBuf,           // required; validated as regular file at probe/start
    pub extra_args: Vec<String>,       // default ["agent", "--no-leader", "stdio"]
    pub request_timeout: Duration,     // default ~180s (legacy)
    pub max_line_bytes: usize,         // default 1 MiB
    pub honor_isolated_home: Option<PathBuf>, // process HOME/USERPROFILE root; NOT a secret value
}

// adapter.rs
pub struct GrokAcpAdapter { /* private fields */ }

impl GrokAcpAdapter {
    pub fn new(config: GrokAcpConfig) -> Result<Self, RuntimeError>;
}

impl AgentRuntimeAdapter for GrokAcpAdapter { ... }
```

**Not public:** ACP session id maps, pending RPC maps, raw JSON messages, child `Child` handle, permission raw params.

### 4.4 Desktop composition types (service ownership)

| Type | Crate / module | B2 change |
| --- | --- | --- |
| `ConversationService` | desktop `conversation_service` | Construct host with `Arc<dyn AgentRuntimeAdapter>` instead of hard-coded `with_fake` |
| `PumpRegistrationHooks` | same | Test-only isolation (§7) |
| Renderer DTOs | `conversation_events` | Unchanged vocabulary |

### 4.5 Types that must **not** be introduced upward

| Rejected public type | Why |
| --- | --- |
| `AcpSessionId` on protocol or DTO | Provider leak |
| `GrokAccount` / lease / slot in kernel | Legacy company runtime model |
| `JsonRpcMessage` in `agent-protocol` | Transport detail |
| Shared “runtime event” that embeds `serde_json::Value` provider blob | Bypasses validate |

---

## 5. Test fixture plan and fake-child process plan

### 5.1 Fixture layers

| Layer | Location | Purpose |
| --- | --- | --- |
| L0 Keeper protocol | `tests/fixtures/protocol/fake-turn.jsonl` (existing) | Unchanged host/domain golden stream |
| L1 ACP wire | `tests/fixtures/protocol/grok-acp/` (new) | NDJSON transcripts of JSON-RPC lines |
| L2 Pairing | same dir `*.map.json` or comments | Documents L1→L0 expected kinds |

Proposed L1 files (small, checked-in, **no secrets**):

```text
tests/fixtures/protocol/grok-acp/
  initialize-ok.jsonl
  session-new-prompt-text-only.jsonl
  session-prompt-with-plan-and-tools.jsonl
  session-cancel-mid-stream.jsonl
  session-prompt-rpc-error.jsonl
  malformed-line.jsonl
  permission-request-read.jsonl
  permission-request-write.jsonl
```

Each successful prompt fixture must end with a prompt **response** line (`stopReason`) after the `session/update` stream so the adapter can emit exactly one terminal.

### 5.2 Unit tests (no process)

| Test | Module | Asserts |
| --- | --- | --- |
| Mapper: every supported `sessionUpdate` → kind + payload passes `validate_payload` | `map/events` | No forbidden keys |
| Mapper: unknown update dropped | `map/events` | No panic, no event |
| Sequence assignment monotonic per turn | `turn` / adapter | 1..n |
| Cancel flag before first emit → single `RunCancelled` | `turn` | One terminal |
| Framer max line exceeded → transport error | `rpc/framer` | No hang |

### 5.3 Fake-child process plan

**Binary:** `fake_acp_agent` (test-only).

Behavior:

1. Read NDJSON JSON-RPC from stdin; write NDJSON to stdout.
2. Script selection via env `KEEPER_FAKE_ACP_SCRIPT` = path to L1 fixture **or** built-in scenario name.
3. On `initialize` → fixed capabilities (`loadSession: false` for minimal; optional true for resume tests).
4. On `session/new` → `{ sessionId: "fake-session-1" }`.
5. On `session/prompt` → emit scripted `session/update` lines with optional per-line delay (`KEEPER_FAKE_ACP_DELAY_MS`); then respond with `{ stopReason: "end_turn" }`.
6. On `session/cancel` / `$/cancel_request` → stop stream; respond to prompt with cancelled stop reason or close mid-flight per scenario.
7. On stdin close → exit 0.
8. Scenario `hang-until-cancel` blocks until cancel for host Drop tests.
9. Scenario `exit-early` exits process to prove `TurnFailed` + adapter cleanup.

**Integration tests** (`adapter_fake_child.rs`):

- Spawn `GrokAcpAdapter` with `executable = fake_acp_agent`.
- Drive `start_session` + `send_turn` into `CollectingSink` / through `RuntimeHost::spawn_turn`.
- Assert event kinds match L0-equivalent ordering for the paired fixture.
- Assert cancel leaves one terminal and `adapter.shutdown()` leaves no child (Windows: process id gone).
- Assert two deterministic runs of the same script produce the same kind sequence (timestamps may differ).

### 5.4 Host and service fixture policy

- Keep `FakeRuntimeAdapter` for host/service regression; **do not** delete B1 tests.
- Add **one** service-level test that injects `GrokAcpAdapter` + fake child (or a thin `Arc<dyn AgentRuntimeAdapter>` mock) to prove composition — without requiring a real Grok install.
- Real Grok binary tests are **optional manual / ignored CI** behind env `KEEPER_GROK_ACP_E2E=1`.

---

## 6. Cancellation, restart, session resume, and shutdown ownership

### 6.1 Cancellation

```text
UI cancelRun
  -> ConversationService::cancel_run
       |-- optional Running -> Cancelling in SQLite
       \-- RuntimeHost::cancel_turn(run_id)
              |-- WorkerRecord.request_cancel + delivery cancel
              \-- adapter.cancel_turn(run_id)
                     |-- ACP session/cancel
                     \-- $/cancel_request(prompt id)

Host worker / ProtocolGuardSink:
  - if cancel wins mid-emit → force RunCancelled, reject late TurnCompleted
  - SpawnedTurn::Drop → cancel_owned_worker_best_effort + join
```

| Step | Owner |
| --- | --- |
| User intent | UI / commands |
| Durable Cancelling / Cancelled status | ConversationService + repository (via events) |
| Worker cancel flag + delivery arbitration | RuntimeHost |
| ACP wire cancel | **Grok adapter** |
| Exactly-one terminal enforcement | ProtocolGuardSink (host) |

Adapter must not write SQLite `Cancelled` itself.

### 6.2 Application restart

| Step | Owner |
| --- | --- |
| On boot `mark_inflight_interrupted()` | ConversationService / repository (B1) |
| Timeline restore from SQLite | ConversationService |
| ACP process from previous boot | **Dead** — adapter does not resurrect foreign PIDs |
| New turns | New `start_session` + new child as needed |

Non-terminal runs become `Interrupted` in SQLite; they are **not** auto-resumed into a live ACP prompt in minimal B2.

### 6.3 Session resume (multi-turn vs durable)

| Mode | B2 minimal | Owner |
| --- | --- | --- |
| **In-process multi-turn** | Reuse adapter-private ACP `sessionId` for the same `conversation_key` after first `session/new` | Adapter map |
| **Durable ACP `session/load` across app restart** | **Deferred** | Would need opaque provider handle in SQLite `runtime_connection` (design §3.5) + `resume_session` or resume field — **not** on B1 trait today |
| **Host `RuntimeSessionId`** | Always Keeper-issued UUID | Protocol |

Do **not** put ACP `sessionId` into `RuntimeSession` or renderer state. If durable resume is added later, store only an opaque `provider_session_ref` blob in a dedicated table via kernel APIs — still never in event payloads.

### 6.4 Shutdown

Existing host order (must stay authoritative):

1. Close sync-turn gate; set shut_down; cancel workers.  
2. Wait admitted sync turns.  
3. `adapter.shutdown()`.  
4. Join workers; assert empty registry / zero slots.

Service order (B1):

1. Close service admission; wait admitted `send_turn`.  
2. `host.shutdown()`.  
3. Drain pumps; require zero residuals.

Adapter `shutdown` responsibilities:

- Cancel active prompts (wire + local flags).  
- Reject new `send_turn`.  
- Close stdin; join reader thread; terminate child process tree (Windows Job Object recommended).  
- Clear session maps.  
- **No** SQLite calls; **no** host worker joins (host owns those).

---

## 7. Isolating public test seams from release builds

### 7.1 Problem

B1 correctly used deterministic seams, but several are **public API in release artifacts**:

- `HostConfig.after_worker_registered`
- `ConversationService::open_with_config_and_pump_hook` + `PumpRegistrationHooks`
- Unlimited constructibility of `FakeRuntimeAdapter` from desktop production path (`RuntimeHost::with_fake`)

### 7.2 Recommended isolation (staged, low churn)

**Option A (preferred for B2):** cargo features

```toml
# runtime-host
[features]
default = []
test-seams = []

# desktop / conversation_service
[features]
default = []
test-seams = ["keeper-runtime-host/test-seams"]
```

- Gate `after_worker_registered` field and `PumpRegistrationHooks` behind `feature = "test-seams"` **or** `cfg(test)`.
- Integration tests enable `test-seams`; release desktop binary does not.
- Keep production defaults behavior-identical when feature off (field absent / constructors absent).

**Option B:** `#[cfg(any(test, feature = "test-seams"))]` on constructors only; keep struct fields private.

**Option C (rejected as sole strategy):** “document that hooks are None in production” — insufficient; release code should not carry injection points.

### 7.3 Fake adapter vs release

| Build | Adapter |
| --- | --- |
| Unit/integration default | `FakeRuntimeAdapter` via feature or test helpers |
| Desktop dev without Grok install | Fake or mock gateway (renderer already has mock) |
| Desktop release / B2 acceptance | `GrokAcpAdapter` only for real-runtime path; fake must not be the only adapter behind a “B2 complete” claim |

Expose a single composition function, e.g. `fn build_runtime_adapter(mode) -> Arc<dyn AgentRuntimeAdapter>`, in desktop **private** modules — not a React-selectable provider matrix (Slice D).

### 7.4 Diagnostics that may remain public

Safe: `active_run_count`, `in_flight_turns`, `probe()` health booleans and version strings.  
Unsafe: any dump of ACP messages, env maps, or home paths to renderer.

---

## 8. Staged implementation plan with disjoint write sets

Workers can proceed in parallel when write sets do not overlap. Each stage ends with `cargo test` for touched packages and Clippy warnings denied.

### Stage B2-0 — Research freeze (this document)

| Write set | Files |
| --- | --- |
| Docs only | `docs/research/2026-07-30-grok-adapter-boundary.md` |

### Stage B2-1 — ACP fixtures + fake child (no product wiring)

| Write set | Files |
| --- | --- |
| Fixtures | `tests/fixtures/protocol/grok-acp/**` |
| Fake child binary | `crates/adapter-grok-acp/tests/bin/fake_acp_agent.rs` (+ cargo bin config) |
| Skeleton crate | `crates/adapter-grok-acp/Cargo.toml`, `src/lib.rs` stubs |

**Does not touch:** `runtime-host`, `agent-protocol`, desktop service.

### Stage B2-2 — Mapper + RPC client (unit-tested)

| Write set | Files |
| --- | --- |
| Adapter crate only | `src/rpc/**`, `src/map/**`, `src/config.rs`, `src/error.rs` |

Prove L1 fixtures map to payloads accepted by `keeper_agent_protocol::validate_payload`.

### Stage B2-3 — Process + `GrokAcpAdapter` trait impl

| Write set | Files |
| --- | --- |
| Adapter crate only | `src/process/**`, `src/turn.rs`, `src/adapter.rs`, integration tests |

Implement full `AgentRuntimeAdapter`. Host tests can use `RuntimeHost::new(Arc::new(adapter), config)` **without** editing host sources.

### Stage B2-4 — Test seam isolation (optional but recommended before release)

| Write set | Files |
| --- | --- |
| Host config only | `crates/runtime-host/src/host/config.rs`, `host/mod.rs` (feature gate) |
| Service hooks only | `conversation_service/types.rs`, `service.rs`, `tests/**` |
| Cargo features | `runtime-host/Cargo.toml`, `src-tauri/Cargo.toml` |

No adapter logic in this stage.

### Stage B2-5 — Desktop composition

| Write set | Files |
| --- | --- |
| Service constructor | `conversation_service/service.rs` (inject `Arc<dyn AgentRuntimeAdapter>`) |
| App state / config load | `app_state.rs`, small config module for executable path |
| Commands | optional `runtime_probe` command |
| **Not** UI provider pickers | — |

Disjoint from B2-2/B2-3 if adapter crate API is stable.

### Stage B2-6 — Acceptance

| Write set | Files |
| --- | --- |
| Verification doc | `docs/verification/2026-07-30-slice-b2-*.md` |
| Optional e2e | desktop e2e with fake child or ignored real Grok |

Gate (design-aligned): real task through Grok Build ACP is required for Stage 1 overall; B2 slice acceptance must name remaining gaps (approvals, durable resume, multi-runtime).

### Parallelism summary

```text
B2-1 ─────────────────────────────┐
B2-2 (after B2-1 skeleton)        │
B2-3 (after B2-2) ────────────────┼──► B2-5 ──► B2-6
B2-4 (independent of B2-2/3) ─────┘
```

---

## 9. Explicit rejected alternatives

| Alternative | Why rejected |
| --- | --- |
| Copy `grok-acp-client.js` / `agent-runtime-adapters.js` into the repo | Violates clean successor + AGENTS.md; brings prompt packets, leases, hooks, company run model |
| Put child process + JSON-RPC inside `RuntimeHost` | Duplicates adapter responsibility; couples host to Grok; breaks Codex/OpenCode symmetry |
| Expand `AgentRuntimeAdapter` with `stream_events` / async channels for B2 | B1 already streams via `EventSink` inside `send_turn`; second path risks double delivery |
| Add ACP types to `keeper-agent-protocol` | Provider leak; pollutes all adapters |
| Let adapter write SQLite or emit Tauri events | Breaks validate→persist→commit→emit authority |
| Use `FakeRuntimeAdapter` as B2 acceptance | Explicitly forbidden by design and B1 acceptance |
| Per-turn new process **and** no session map as the only long-term model | Acceptable emergency fallback; poor multi-turn context; prefer long-lived stdio + `session/new` |
| Serialize all host concurrency inside adapter ignoring `RuntimeHost` limits | Two controllers; cancel/shutdown races |
| Auto-approve all `session/request_permission` | Safety regression; contradicts Ask-default design |
| Port legacy isolated account lease pool into kernel | Stage 3 / legacy company concern; Stage 1 uses OS keychain + explicit config later |
| Store ACP `sessionId` on `RuntimeSession` or in event payloads | Upward provider leak |
| Shared mutable JSON files for session state | Domain state belongs in SQLite; provider state stays in-process in the adapter |
| Renderer-selectable model/account UI in B2 | Slice D; B1 UI rule is no provider controls |
| Unbounded stdout buffer | Legacy caps max line / response; keep caps |
| Implement durable `session/load` before opaque storage exists | Would force ACP ids into wrong layers |

---

## 10. Minimal B2 success criteria (architecture view)

B2 adapter work is architecturally sound when:

1. `GrokAcpAdapter: AgentRuntimeAdapter` lives in `keeper-adapter-grok-acp` only.  
2. Host and conversation service gain **no** ACP types.  
3. Fixture + fake-child tests prove mapping, cancel, and process cleanup without a real Grok install.  
4. Provider session ids, env homes, and RPC payloads never appear in SQLite event payloads or renderer DTOs.  
5. Shutdown ownership remains: service admission → host → adapter process teardown → pump drain.  
6. Fake adapter remains available for B1 regressions but is not the B2 acceptance runtime.  
7. Every new source file is under 500 lines; write sets follow §8.

---

## 11. Source inventory used for this map

**Successor (this worktree):**

- `AGENTS.md`
- `docs/superpowers/specs/2026-07-29-pillow-first-design.md`
- `docs/superpowers/plans/2026-07-30-slice-b1-conversation-kernel.md`
- `docs/verification/2026-07-30-slice-b1-acceptance.md`
- `crates/agent-protocol/src/**`
- `crates/runtime-host/src/**`
- `apps/desktop/src-tauri/src/conversation_service/**`
- `tests/fixtures/protocol/fake-turn.jsonl`

**Legacy reference only (not modified):**

- `legacy Keeper (out of tree)\lib\grok-acp-client.js`
- `legacy Keeper (out of tree)\lib\agent-runtime-adapters.js`
- `legacy Keeper (out of tree)\lib\keeper-acp-tool-policy.js` (permission observe path)

**External protocol vocabulary:** Agent Client Protocol v1 (`initialize`, `session/*`, `session/update` variants) as documented by the ACP project; legacy client covers a deliberate subset (`agent_message_chunk`, `tool_call`, `tool_call_update`).
