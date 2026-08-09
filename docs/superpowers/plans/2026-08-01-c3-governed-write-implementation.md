# Pillow C3 Governed Write Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use test-driven development for
> the assigned task. Read only the extracted task brief, obey its file
> whitelist, and report changed files, tests, concerns and `COMPLETION: DONE`.

**Goal:** Deliver a real provider-to-human-to-host single-file UTF-8 governed
write loop with bounded diff, one-shot approval, atomic apply, restart safety
and conversation-owned UI.

**Architecture:** Extend the accepted B3 reservation/receipt spine with a
dedicated write proposal ledger and `kernel-policy` approval state machine.
The provider supplies no filesystem or approval authority; the host captures
and seals the active payload in memory, stores immutable identity and digests,
computes the diff and applies only after a single-use human grant passes
base-hash CAS.

**Tech Stack:** Rust 2024, rusqlite, Tauri 2, React 19, TypeScript 5.8, Vitest,
Node desktop E2E, Windows WebView2.

**Final status (2026-08-02):** Tasks 1-7 are accepted by the controller after
adversarial correction and full real-desktop/repository gates. C3 is recorded
as an unstaged, uncommitted working-tree checkpoint. C4 has not started.

## Global Constraints

- Baseline HEAD is `7b7f70f`; preserve the accepted unstaged C1/B3/C2 tree.
- Do not reset, discard, stage, commit, package or publish.
- Do not modify `legacy Keeper (out of tree)`.
- C3.0 supports only one UTF-8 `create` or `modify` path per proposal.
- Proposed and base content cap: exactly 262,144 UTF-8 bytes.
- Diff cap: exactly 262,144 bytes and 4,000 lines with a truncation marker.
- Proposal TTL: exactly 600 seconds.
- Every write requires `approve_once`; no session or persistent auto-allow.
- Parent directory must already exist; no directory mutation.
- Renderer/provider never supply roots, base content, digests or apply content.
- Terminal, Git, network, credentials, Obsidian and new runtimes remain excluded.
- Every worker boots Keeper first and writes only its brief/report whitelist.
- Each task ends with focused tests, formatting/diff checks and a report; the
  controller owns integration and final acceptance.

---

### Task 1: Protocol And Approval Kernel

**Owner:** grok3

**Files:**

- Create: `crates/kernel-policy/Cargo.toml`
- Create: `crates/kernel-policy/src/lib.rs`
- Create: `crates/kernel-policy/src/write.rs`
- Create: `crates/kernel-policy/tests/write_policy.rs`
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/agent-protocol/src/tool/types.rs`
- Modify: `crates/agent-protocol/src/tool/manifest.rs`
- Modify: `crates/agent-protocol/src/tool/validate.rs`
- Modify: `crates/agent-protocol/src/tool/public_error.rs`
- Modify: `crates/agent-protocol/src/lib.rs`
- Modify: `crates/agent-protocol/tests/tool_contract.rs`
- Modify: `crates/agent-protocol/tests/tool_contract_security.rs`

**Interfaces:**

```rust
pub enum WriteMode { Create, Modify }
pub struct FsWriteArguments { pub path: String, pub mode: WriteMode, pub content: String }
pub struct FsWriteOutput { pub path: String, pub mode: WriteMode, pub bytes_written: u64, pub content_digest: String }
pub enum WriteProposalState { PendingApproval, Approved, Applying, Succeeded, Rejected, Expired, Cancelled, Interrupted, Failed }
pub enum WriteDecision { ApproveOnce, Reject }
pub const WRITE_CONTENT_BYTE_CAP: u64 = 262_144;
pub const WRITE_PROPOSAL_TTL_SECS: i64 = 600;
```

- [ ] Add failing protocol tests for exact `fs.write@1` schema, byte cap, NUL,
  portable path, unknown fields, version and digest mismatch.
- [ ] Run `cargo test -p keeper-agent-protocol --tests`; confirm the new cases
  fail because `fs.write` is not yet in the fixed catalog.
- [ ] Add `ToolId::FsWrite`, typed arguments/output, manifest and strict result
  validation without weakening existing read cases.
- [ ] Add fixed public codes for approval required/rejected/expired,
  base-changed, write-conflict, path-rejected and write-failed; wire parsing must
  reject non-canonical custom messages.
- [ ] Add failing kernel-policy tests for allowed transitions, expiry at the
  exact boundary, terminal non-reopen and approve/reject single-use behavior.
- [ ] Implement the pure policy state machine with no filesystem, renderer or
  adapter dependency.
- [ ] Run focused protocol and policy tests, then
  `cargo fmt --all -- --check` and `git diff --check`.
- [ ] Write the task report; do not stage or commit.

### Task 2: Durable Write Proposal Ledger

**Owner:** grok1

**Depends on:** Task 1 accepted interfaces.

**Files:**

- Create: `crates/storage-sqlite/migrations/0005_write_proposals.sql`
- Create: `crates/storage-sqlite/src/write_proposal.rs`
- Create: `crates/storage-sqlite/tests/write_proposal_repository.rs`
- Modify: `crates/storage-sqlite/Cargo.toml`
- Modify: `crates/storage-sqlite/src/lib.rs`
- Modify: `crates/storage-sqlite/src/migrations.rs`
- Modify: `crates/storage-sqlite/tests/migrations.rs`

**Interfaces:**

```rust
pub struct StoredWriteProposal {
    pub proposal_id: String,
    pub reservation_key: String,
    pub proposal_fingerprint: String,
    pub project_id: String,
    pub conversation_id: String,
    pub run_id: RunId,
    pub call_id: String,
    pub path: String,
    pub mode: WriteMode,
    pub proposed_digest: String,
    pub proposed_bytes: u64,
    pub base_digest: Option<String>,
    pub base_bytes: u64,
    pub state: WriteProposalState,
    pub expires_at: DateTime<Utc>,
    pub result_json: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
pub enum WriteReservationOutcome { Reserved(StoredWriteProposal), Replay(StoredWriteProposal), Conflict(StoredWriteProposal), Busy(StoredWriteProposal) }
pub enum WriteDecisionOutcome { Accepted(StoredWriteProposal), Terminal(StoredWriteProposal), Conflict(StoredWriteProposal), Expired(StoredWriteProposal) }
pub struct SqliteWriteProposalRepository;
```

- [ ] Write migration tests for table/check/FK/unique constraints and verify
  migration order through `0005`.
- [ ] Write failing repository tests for reserve/replay/conflict, one pending
  proposal per run, one decision/grant use, expiry, transition history,
  apply lease, terminal receipt and restart interruption.
- [ ] Implement `0005` without changing historical migration SQL.
- [ ] Implement transactionally checked project/conversation/run ownership;
  never trust caller identity joins.
- [ ] Store proposal metadata/digests and grant digest, never proposed/base
  content or the raw grant. Add a regression that distinctive secret-bearing
  proposal text is absent from the SQLite file and repository DTOs.
- [ ] Make recovery terminalize pending/approved rows without applying or
  deleting evidence. Keep applying rows available for read-only reconciliation.
- [ ] Run `cargo test -p keeper-storage-sqlite --test write_proposal_repository`
  and the complete storage package tests.
- [ ] Run formatting and diff checks; write report; no stage/commit.

### Task 3: Host Diff And Atomic Write Executor

**Owner:** grok3

**Depends on:** Tasks 1–2 accepted.

**Files:**

- Create: `crates/tool-executor/src/write.rs`
- Create: `crates/tool-executor/src/write_diff.rs`
- Create: `crates/tool-executor/tests/governed_write.rs`
- Create: `crates/tool-executor/tests/write_diff.rs`
- Modify: `crates/tool-executor/Cargo.toml`
- Modify: `crates/tool-executor/src/lib.rs`

**Interfaces:**

```rust
pub struct BoundedDiff {
    pub unified: String,
    pub truncated: bool,
    pub line_count: u32,
    pub byte_count: u64,
}
pub struct PreparedWriteProposal {
    pub metadata: StoredWriteProposal,
    pub base_content: Option<String>,
    pub proposed_content: String,
    pub diff: BoundedDiff,
}
pub struct GovernedWriteToolGateway;
pub enum GovernedWriteOutcome { Pending(WriteProposalView), Replayed(ValidatedToolResult) }
pub fn build_bounded_unified_diff(base: Option<&str>, proposed: &str) -> BoundedDiff;
```

- [ ] Write failing tests for create/modify success, create-existing,
  modify-missing, binary/oversize base, base drift, denied credential paths,
  dot/device/ADS paths, file/directory/symlink/reparse targets, cancellation,
  concurrent apply and temp cleanup.
- [ ] Write failing diff tests for create/modify, empty/final-newline, Unicode,
  deterministic hunks and exact byte/line truncation.
- [ ] Implement host base capture and SHA-256 content digests.
- [ ] Implement denied components/names/extensions exactly as the design.
- [ ] Implement same-directory `create_new` temp writes, flush/sync, repeated
  validation and atomic create/replace. Windows modification must use an atomic
  OS replacement primitive; never remove target then rename.
- [ ] Verify the sealed active-host payload against repository metadata/digests
  before apply; ignore any content supplied by decision callers.
- [ ] Persist terminal receipt before returning success.
- [ ] Run all tool-executor tests plus protocol/storage regressions, fmt and
  diff checks; write report; no stage/commit.

### Task 4: Runtime, Adapter And Native Commands

**Owner:** grok1

**Depends on:** Tasks 1–3 accepted.

**Files:**

- Create: `crates/runtime-host/src/governed_write.rs`
- Create: `crates/runtime-host/tests/governed_write_bridge.rs`
- Create: `crates/adapter-grok-acp/src/provider_writes.rs`
- Create: `crates/adapter-grok-acp/tests/governed_write_roundtrip.rs`
- Create: `apps/desktop/src-tauri/src/write_commands.rs`
- Create: `apps/desktop/src-tauri/src/conversation_service/write_service.rs`
- Create: `apps/desktop/src-tauri/src/write_commands_tests.rs`
- Modify: `crates/runtime-host/src/lib.rs`
- Modify: `crates/runtime-host/src/host/mod.rs`
- Modify: `crates/adapter-grok-acp/src/lib.rs`
- Modify: `crates/adapter-grok-acp/src/prompt.rs`
- Modify: `crates/adapter-grok-acp/src/bin/fake_acp_agent/scripts.rs`
- Modify: `apps/desktop/src-tauri/src/conversation_service/mod.rs`
- Modify: `apps/desktop/src-tauri/src/conversation_service/service.rs`
- Modify: `apps/desktop/src-tauri/src/conversation_service/reconciliation.rs`
- Modify: `apps/desktop/src-tauri/src/app_state.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Native command names and payload:**

```text
list_write_proposals(conversationId) -> WriteProposalDto[]
decide_write_proposal(conversationId, proposalId, approvalGrant, decision)
  -> WriteProposalDto
open_write_proposal_diff(conversationId, proposalId) -> WriteDiffDto
```

- [ ] Add RED provider fixtures for strict `pillow/write_tool`, foreign
  session, unknown fields/tools/version, conflicting transport identity and
  provider-native write permission denial.
- [ ] Add RED runtime tests for run binding, durable pending-before-event,
  exact-run wakeup, cancel/expiry, other-conversation independence, replay and
  restart interruption.
- [ ] Add RED native command tests proving foreign conversation/project and
  altered content/digests cannot decide or apply.
- [ ] Implement the run-scoped coordinator. The waiting request checks cancel
  and expiry without busy-spinning and wakes only for its proposal.
- [ ] Emit summary-only `write_proposal_requested` and
  `write_proposal_resolved` events; never place content/grants/native paths in
  `run_events`.
- [ ] Keep ACP `session/request_permission` write decisions denied.
- [ ] Implement command DTOs with portable paths and fixed public errors.
- [ ] Ensure shutdown/recovery interrupts pending/approved proposals. Reconcile
  an `applying` row read-only: proposed digest means recovered success, base
  digest/absence means interrupted, anything else means opaque failure. Leave
  no detached waiter/process.
- [ ] Run focused runtime/adapter/native tests, then all affected package tests,
  fmt and diff checks; write report; no stage/commit.

### Task 5: Conversation Approval And Diff UI

**Owner:** grok2

**May begin after Task 4 command DTO is frozen. No native Rust files.**

**Files:**

- Create: `apps/desktop/src/lib/writeApprovalGateway.ts`
- Create: `apps/desktop/src/lib/tauriWriteApprovalGateway.ts`
- Create: `apps/desktop/src/features/conversations/WriteApprovalBar.tsx`
- Create: `apps/desktop/src/features/workbench/WriteDiffPreview.tsx`
- Create: `apps/desktop/src/styles/write-approval.css`
- Create: `apps/desktop/tests/write-approval-gateway.test.ts`
- Create: `apps/desktop/tests/write-approval-bar.test.tsx`
- Create: `apps/desktop/tests/write-diff-preview.test.tsx`
- Create: `apps/desktop/tests/write-approval-workbench.test.tsx`
- Modify: `apps/desktop/src/lib/conversationGateway.ts`
- Modify: `apps/desktop/src/features/conversations/agentActivityTypes.ts`
- Modify: `apps/desktop/src/features/conversations/agentActivityFromTimeline.ts`
- Modify: `apps/desktop/src/features/conversations/agentActivityProjection.ts`
- Modify: `apps/desktop/src/features/conversations/ConversationTimeline.tsx`
- Modify: `apps/desktop/src/features/workbench/Composer.tsx`
- Modify: `apps/desktop/src/features/workbench/ResourcePane.tsx`
- Modify: `apps/desktop/src/features/workbench/Workbench.tsx`
- Modify: `apps/desktop/src/main.tsx`

- [ ] Write RED gateway tests for exact invoke payloads, opaque errors and no
  root/content/digest fields.
- [ ] Write RED component tests for conversation filtering, one active bar,
  initial focus, Tab trap, Escape reject, expiry, disabled submit, terminal
  reconciliation and foreign proposal absence.
- [ ] Write RED diff tests for portable path, explicit truncation and no raw
  native paths/IDs/provider output.
- [ ] Implement a single approval bar above the composer. No permanent card
  stack and no allow-session option.
- [ ] Open the host diff in the existing collapsible contextual pane; preserve
  conversation minimum width and project-switch generation guards.
- [ ] Project terminal outcomes into the owning timeline and expose `Open file`
  only after success.
- [ ] Run focused Vitest, desktop typecheck, production UI build and diff check;
  write report; no stage/commit.

### Task 6: Real Desktop Integration And Adversarial Acceptance

**Owner:** grok3 for harness; controller owns verdict.

**Files:**

- Create: `apps/desktop/tests/e2e/c3-governed-write.spec.ts`
- Create: `scripts/wave-c3-governed-write-acceptance.mjs`
- Create: `scripts/lib/c3-governed-write-acceptance.mjs`
- Create: `docs/execution/2026-08-01-wave-c3-integration-report.md`
- Modify: `apps/desktop/package.json`

- [x] Drive a real fake-ACP proposal through the production Tauri/WebView2
  command path; do not mutate project fixtures behind the UI.
- [x] Prove reject, approve-create, approve-modify, base drift, expiry,
  cancellation and conflicting replay.
- [x] Prove two projects/two conversations do not leak approval, diff, content
  or outcome.
- [x] Kill/restart with a pending proposal; verify `interrupted` and zero disk
  mutation, then verify a terminal success restores as history.
- [x] Capture supported Windows screenshots and assert conversation remains the
  visual center with the resource pane closed/open.
- [x] Verify normal close and relevant Pillow/fake-ACP/Cargo/Rust cleanup.
- [x] Run focused C3 E2E serially and write the integration report.

### Task 7: Controller Acceptance And Checkpoint

**Owner:** Codex Desktop controller. Grok4 and account-2 Codex CLI own bounded
development only; the controller owns the final verdict.

- [x] Inspect every changed file against the spec and worker white-lists.
- [x] Run focused protocol, policy, storage, executor, runtime, adapter, native,
  renderer and C3 desktop tests personally.
- [x] Run `pnpm check`.
- [x] Run `pnpm --filter @pillow/desktop build:ui`.
- [x] Run `pnpm desktop:test:e2e` serially.
- [x] Run `cargo fmt --all --check`, `git diff --check`, and cached diff
  check.
- [x] Verify HEAD unchanged, staging empty, no unrelated worktree loss and no
  relevant residual processes.
- [x] Use Grok4 and the independent account-2 Codex CLI for bounded development;
  controller-review every changed line and rerun the affected gates.
- [x] Record `docs/verification/2026-08-02-wave-c3-acceptance.md` and
  `docs/verification/2026-08-02-wave-c3-checkpoint.md` only if the controller
  verdict is ACCEPT.
- [x] Update README and Stage 1 overall status; send Keeper completion summary;
  do not stage or commit.
