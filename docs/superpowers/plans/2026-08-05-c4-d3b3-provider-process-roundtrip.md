# C4 D3B3 Provider Process Roundtrip Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the real provider-to-runtime governed process path that must exist before Pillow can claim a desktop process E2E.

**Architecture:** The Grok ACP adapter owns strict provider JSON-RPC normalization and provider-safe responses. `keeper-runtime-host` remains the only process proposal, approval, execution, replay, and output authority. The desktop conversation service binds the admitted run to that authority using the same project, conversation, and root already used for governed reads; failure rolls the full binding back before the adapter session starts.

**Tech Stack:** Rust, JSON-RPC over NDJSON, Pillow runtime-host governed process API, fake ACP agent integration fixtures, SQLite-backed desktop conversation service.

## Global Constraints

- Work only in `<repo>`; `<workspace>/VaultSystem\keeper` is record-only and must not provide prompts, context, decisions, or code.
- Do not stage, commit, reset, restore, clean, or overwrite unrelated dirty work.
- Only touch the exact Task 5A allowlist in the execution brief.
- Preserve runtime-host as the sole process ledger/execution authority; do not create another database/repository/channel or expose approval grants.
- Provider errors and responses must not expose absolute native paths, grants, captured stdout/stderr, environment data, raw session IDs, or debug metadata.
- Use TDD. Record the first RED failure and the final GREEN commands in the report.

---

### Task 1: Strict provider process normalizer

**Files:**
- Create: `crates/adapter-grok-acp/src/provider_processes.rs`
- Modify: `crates/adapter-grok-acp/src/lib.rs`
- Test: `crates/adapter-grok-acp/tests/governed_process_roundtrip.rs`

**Interfaces:**
- Consumes JSON-RPC method `pillow/process_tool` and params `{ sessionId, callId, toolId, toolVersion, arguments }`.
- Produces `keeper_runtime_host::ProviderProcessProposal` only when `sessionId` equals the active ACP session, `callId` is non-empty, `toolId == "process.run"`, `toolVersion == 1`, and `arguments` contains exactly `program`, `argv`, and optional `cwd` with the runtime-host accepted types.
- Produces only fixed provider-safe JSON success metadata or fixed JSON-RPC error code/message pairs.

- [ ] **Step 1: Write failing normalization and response tests**

Cover accepted portable argv/cwd; wrong session; empty call ID; unknown/extra fields; wrong tool/version; non-string program/argv/cwd; rejected, expired, cancelled, timed-out, failed, busy, conflict, and replay outcomes. Assert no response contains grant material, output bytes, absolute native paths, or raw internal errors.

- [ ] **Step 2: Run the focused test to prove RED**

Run: `cargo test -p keeper-adapter-grok-acp --test governed_process_roundtrip -- --nocapture`

Expected: failure because the process normalizer/handler does not exist.

- [ ] **Step 3: Implement the smallest strict normalizer and response mapper**

Use a dedicated request error enum with only `MethodNotFound` and `InvalidParams`. Reject unknown object keys. Return `ProviderProcessProposal { call_id, tool_id, tool_version, arguments }`; do not resolve filesystem authority in the adapter. Response success may contain only terminal execution metadata from `ProcessRunOutput` plus a replay marker if required by the existing receipt contract.

- [ ] **Step 4: Run the focused test**

Run: `cargo test -p keeper-adapter-grok-acp --test governed_process_roundtrip -- --nocapture`

Expected: normalization/response cases pass; roundtrip cases may remain RED until Tasks 2-3.

### Task 2: ACP process handler and durable event order

**Files:**
- Modify: `crates/adapter-grok-acp/src/prompt.rs`
- Test: `crates/adapter-grok-acp/tests/governed_process_roundtrip.rs`

**Interfaces:**
- Consumes the normalized proposal and active run ID.
- Calls only `execute_governed_process(run_id, proposal, cancel, on_requested)`.
- Emits fresh execution events in exact durable sequence: requested, decided, started, terminal. Rejection/expiry omit started. Replay emits no duplicate request/decision/start/terminal sequence and never spawns again.

- [ ] **Step 1: Add failing handler tests**

Test approve-once, reject, exact replay, fingerprint conflict, cancel/timeout mapping, event ordering, and provider completion after the terminal event. Assert one approval creates at most one spawn and replay returns the same terminal metadata without duplicate events.

- [ ] **Step 2: Implement `pillow/process_tool` dispatch before the governed-read normalizer**

Mirror the established write handler's active-run and sequence allocation pattern. Emit requested inside `on_requested`; after an executed receipt, allocate and emit the optional decided, started, and terminal payloads in order. For `GovernedProcessDelivery::Replayed`, respond from the receipt without re-emitting durable events. Map bridge errors to fixed public error codes/messages.

- [ ] **Step 3: Run the focused roundtrip test**

Run: `cargo test -p keeper-adapter-grok-acp --test governed_process_roundtrip -- --nocapture`

Expected: all cases pass with no duplicate event or spawn.

### Task 3: Fake ACP governed-process scenario

**Files:**
- Modify: `crates/adapter-grok-acp/src/bin/fake_acp_agent/main.rs`
- Modify: `crates/adapter-grok-acp/src/bin/fake_acp_agent/scripts.rs`
- Test: `crates/adapter-grok-acp/tests/governed_process_roundtrip.rs`

**Interfaces:**
- Adds named fake scripts that issue `pillow/process_tool` and wait for the matching provider response.
- Uses portable commands/arguments only; no machine-specific absolute paths.

- [ ] **Step 1: Add fake scripts for approve/reject/replay/conflict**

Each script sends the exact process RPC shape and emits only a deterministic terminal label after it receives the provider response. The replay script reuses the same call ID and identical arguments; the conflict script reuses the call ID with changed arguments.

- [ ] **Step 2: Prove the adapter package integration**

Run: `cargo test -p keeper-adapter-grok-acp --all-targets -- --nocapture`

Expected: all adapter unit, protocol fixture, and fake-child integration tests pass.

### Task 4: Desktop conversation process-run binding

**Files:**
- Modify: `apps/desktop/src-tauri/src/conversation_service/service.rs`
- Modify only if a focused test requires registration: `apps/desktop/src-tauri/src/conversation_service/mod.rs`
- Create only if the existing conversation test module cannot house the case cleanly: `apps/desktop/src-tauri/src/conversation_service/process_binding_tests.rs`

**Interfaces:**
- After `register_governed_read_run` succeeds, calls `register_governed_process_run` with the same protocol run ID, project ID, conversation ID, and resolved project root.
- If process registration fails, calls the existing release path before failing the admitted run with fixed code `governed_process_unavailable`.
- Existing session-start, spawn, pump-terminal, cancel, and shutdown release paths must continue to release read/write/process bindings exactly once through the established combined release method.

- [ ] **Step 1: Add a focused fail-closed binding test if an existing seam supports it**

The test must prove the adapter session is not started when process registration fails and no run binding remains. Do not add a production-only failure switch.

- [ ] **Step 2: Add process registration and rollback**

Keep registration immediately beside governed-read registration. Preserve current durable admission and terminalization ordering. Do not expose the runtime host or process gateway to the renderer.

- [ ] **Step 3: Run Rust acceptance gates**

Run:

```powershell
cargo test -p keeper-adapter-grok-acp --all-targets -- --nocapture
cargo test -p pillow-desktop --all-targets -- --nocapture
cargo check --workspace --all-targets
cargo clippy -p keeper-adapter-grok-acp -p pillow-desktop --all-targets -- -D warnings
cargo fmt --all -- --check
```

Expected: every command exits 0.

- [ ] **Step 4: Write the execution report and verify scope**

Create `docs/execution/2026-08-05-wave-c4-d3b3-codex2-provider-process-roundtrip-report.md` with changed files, RED evidence, GREEN evidence, event/replay guarantees, residual risks, and `completion: complete|blocked`.

Run: `git diff --check -- <exact allowlist paths>`

Expected: exit 0. Do not stage or commit.

