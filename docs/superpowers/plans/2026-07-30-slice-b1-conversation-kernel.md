# Pillow Slice B1 Conversation Kernel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a persistent, streaming and cancellable conversation vertical
slice through a deterministic fake runtime, without coupling Keeper domain
state to Grok, Codex or OpenCode.

**Architecture:** Rust owns conversation, run and event state. SQLite stores
ordered normalized events. A runtime-neutral host consumes commands and emits
events. React projects those events into the Grok-style conversation surface
through a typed gateway. The fake adapter proves the boundary before real ACP.

**Tech Stack:** Rust 2024, rusqlite, serde, Tauri 2, React 19, TypeScript,
Vitest, deterministic protocol fixtures.

## Global Constraints

- The project owns every conversation and run.
- SQLite is the only operational authority.
- Renderer code cannot open SQLite, launch processes or access project files.
- Provider-specific payloads cannot cross the adapter boundary.
- Event ordering is monotonic per run and replay is idempotent.
- Secrets cannot enter events, logs, SQLite or renderer state.
- A fake adapter is test evidence only and cannot satisfy Slice B2.
- No Obsidian, terminal, Git, editor, account, quota or provider-routing UI.
- Existing Slice A project flows and visual screenshots must remain unchanged.
- Every introduced source file stays below 500 lines.

## Fixed Domain Vocabulary

```rust
pub enum RunStatus {
    Pending,
    Running,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

pub enum RuntimeEventKind {
    RunStarted,
    AssistantDelta,
    ReasoningSummaryDelta,
    PlanUpdated,
    ToolCallStarted,
    ToolCallOutput,
    ToolCallCompleted,
    SubagentStarted,
    SubagentProgress,
    SubagentCompleted,
    ArtifactProduced,
    TurnCompleted,
    TurnFailed,
    RunCancelled,
    Heartbeat,
}
```

Every persisted run event has:

```text
event_id, run_id, sequence, kind, payload_json, created_at
```

`(run_id, sequence)` is unique. Duplicate event replay returns the existing
record and cannot duplicate timeline content.

## Task 1: Conversation Domain And SQLite Authority

**Owner:** Grok worker A

**Files:**

- Create: `crates/kernel-session/Cargo.toml`
- Create: `crates/kernel-session/src/lib.rs`
- Create: `crates/storage-sqlite/migrations/0002_conversations.sql`
- Create: `crates/storage-sqlite/tests/conversation_repository.rs`
- Modify: `crates/storage-sqlite/src/lib.rs`
- Modify: `crates/storage-sqlite/src/migrations.rs`
- Modify: `Cargo.toml`

**Produces:**

```rust
pub trait ConversationRepository {
    fn create_conversation(&self, input: NewConversation) -> Result<Conversation, SessionError>;
    fn list_conversations(&self, project_id: ProjectId) -> Result<Vec<Conversation>, SessionError>;
    fn create_run(&self, input: NewRun) -> Result<Run, SessionError>;
    fn append_event(&self, event: NewRunEvent) -> Result<RunEvent, SessionError>;
    fn load_timeline(&self, conversation_id: ConversationId) -> Result<Timeline, SessionError>;
    fn transition_run(&self, run_id: RunId, next: RunStatus) -> Result<Run, SessionError>;
    fn mark_inflight_interrupted(&self) -> Result<usize, SessionError>;
}
```

- [ ] Write failing kernel tests for legal/illegal run transitions.
- [ ] Write failing migration/repository tests for project isolation, event
  ordering, duplicate replay and restart restoration.
- [ ] Implement only the interfaces listed in this task; do not add search,
  export, approvals or provider metadata.
- [ ] Verify migration 0001 data remains readable after migration 0002.
- [ ] Run `cargo test -p keeper-session -p keeper-storage-sqlite`.
- [ ] Commit with a semantic message and write the worker report.

## Task 2: Runtime-Neutral Protocol And Deterministic Host

**Owner:** Grok worker B

**Files:**

- Create: `crates/agent-protocol/Cargo.toml`
- Create: `crates/agent-protocol/src/lib.rs`
- Create: `crates/runtime-host/Cargo.toml`
- Create: `crates/runtime-host/src/lib.rs`
- Create: `crates/runtime-host/tests/fake_runtime.rs`
- Create: `tests/fixtures/protocol/fake-turn.jsonl`
- Modify: `Cargo.toml`

**Produces:**

```rust
pub trait AgentRuntimeAdapter: Send + Sync {
    fn probe(&self) -> Result<RuntimeProbe, RuntimeError>;
    fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError>;
    fn send_turn(&self, request: SendTurn, sink: EventSink) -> Result<(), RuntimeError>;
    fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError>;
    fn shutdown(&self) -> Result<(), RuntimeError>;
}
```

The deterministic fake adapter emits the fixed JSONL fixture with bounded
delays and cooperatively stops after cancellation.

- [ ] Write failing serde round-trip tests for every fixed event kind.
- [ ] Write failing host tests for ordered streaming, cancellation,
  single-terminal-event enforcement and shutdown cleanup.
- [ ] Implement protocol types without provider fields.
- [ ] Implement a bounded fake runtime; no shell process and no network.
- [ ] Prove fixture replay is deterministic across two runs.
- [ ] Run the crate tests and Clippy with warnings denied.
- [ ] Commit with a semantic message and write the worker report.

## Task 3: Conversation UI And Typed Gateway

**Owner:** Grok worker C

**Files:**

- Create: `apps/desktop/src/features/conversations/ConversationTimeline.tsx`
- Create: `apps/desktop/src/features/conversations/RunActivity.tsx`
- Create: `apps/desktop/src/features/conversations/conversationReducer.ts`
- Create: `apps/desktop/src/lib/conversationGateway.ts`
- Create: `apps/desktop/src/lib/mockConversationGateway.ts`
- Create: `apps/desktop/tests/conversation-workbench.test.tsx`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/features/projects/AppSidebar.tsx`
- Modify: `apps/desktop/src/features/workbench/Composer.tsx`
- Modify: `apps/desktop/src/features/workbench/Workbench.tsx`
- Modify: `apps/desktop/src/styles/workbench-shell.css`
- Modify: `apps/desktop/src/styles/workbench-chrome.css`

**Consumes:** TypeScript mirrors of the fixed domain vocabulary in this plan.

**Produces:**

```ts
export interface ConversationGateway {
  listConversations(projectId: string): Promise<ConversationDto[]>;
  createConversation(projectId: string, title?: string): Promise<ConversationDto>;
  loadTimeline(conversationId: string): Promise<TimelineDto>;
  sendTurn(conversationId: string, content: string): Promise<{ runId: string }>;
  cancelRun(runId: string): Promise<void>;
  subscribe(listener: (event: RuntimeEventDto) => void): () => void;
}
```

- [ ] Write failing tests for project-isolated conversation lists.
- [ ] Write failing tests for send, streaming deltas, plan/tool/sub-agent
  activity, completion and cancellation.
- [ ] Write a failing test proving a stream continues while another
  conversation is selected.
- [ ] Implement the reducer and deterministic mock gateway.
- [ ] Adapt the existing Grok-style sidebar, timeline and composer without
  adding provider/model controls.
- [ ] Verify composer geometry does not move during streaming.
- [ ] Run renderer typecheck, tests and production build.
- [ ] Regenerate empty/selected screenshots and prove Slice A hashes remain
  unchanged where the conversation state is absent.
- [ ] Commit with a semantic message and write the worker report.

## Task 4: Integration And Slice B1 Acceptance

**Owner:** Keeper controller plus independent reviewer

**Files:**

- Create: `apps/desktop/src-tauri/src/conversation_commands.rs`
- Create: `apps/desktop/src-tauri/src/conversation_events.rs`
- Create: `apps/desktop/src-tauri/src/conversation_service.rs`
- Create: `apps/desktop/src/lib/tauriConversationGateway.ts`
- Create: `apps/desktop/tests/tauri-conversation-gateway.test.ts`
- Create: `apps/desktop/tests/e2e/conversation-recovery.spec.ts`
- Create: `docs/verification/2026-07-30-slice-b1-acceptance.md`
- Modify: `Cargo.toml`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/src/app_state.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src/main.tsx`
- Modify: `apps/desktop/package.json`
- Modify: `package.json`

- [ ] Integrate Task 1, then Task 2, then Task 3.
- [ ] Resolve only interface-level merge conflicts; do not redesign accepted
  worker code during integration.
- [ ] Connect host events to repository transactions before emitting renderer
  events.
- [ ] Add a real desktop acceptance scenario: create project, create
  conversation, send fake turn, observe stream, restart, restore timeline.
- [ ] Add cancellation acceptance and prove no runtime/process residual.
- [ ] Run `pnpm check`, native acceptance, release build and release smoke.
- [ ] Run independent spec review followed by code-quality review.
- [ ] Fix every Critical/Important finding and re-review.
- [ ] Record that Slice B1 is deterministic-host acceptance, not real Grok ACP.

## Slice B1 Exit Gate

Slice B1 passes only when:

1. two projects retain isolated conversation lists;
2. ordered fake-runtime events stream into the selected conversation;
3. switching conversations does not stop an active run;
4. cancellation produces exactly one terminal state;
5. restart restores messages, run status and timeline from SQLite;
6. non-terminal runs become `Interrupted` on host restart;
7. all runtime and desktop processes are cleaned;
8. the UI contains no provider-specific state;
9. the report names all unimplemented Slice B2 behavior.
