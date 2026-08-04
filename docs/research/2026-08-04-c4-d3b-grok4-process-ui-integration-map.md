# C4 D3B — Governed process UI integration map (Grok4 research)

Date: 2026-08-04
Worker: grok4 (Keeper session `<keeper-session>`)
Lease: `<keeper-lease>` (exact release target)
Task: `c4-d3b-process-ui-integration-research`
Brief: `docs/execution/2026-08-04-wave-c4-d3b-grok4-integration-research-brief.md`
Status: preparation only — **no product implementation**

## Repo evidence lock

| Check | Result |
| --- | --- |
| `git rev-parse HEAD` | `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b` |
| `git diff --cached --name-only` | empty |
| Write allowlist | this file only |
| Unrelated dirty tree | preserved (C1–C4 working tree) |

## Executive summary

D3B can integrate **conversation-owned process proposal → decision → live output → terminal outcome** into the existing Tauri/TS/React workbench **without a second dashboard**, by reusing C3 write-approval placement and C2 progressive-disclosure patterns.

**What is already built (dirty tree, not all controller-accepted):**

1. Domain authority: `process.run@1`, policy, supervised process executor, exec ledger, sealed gateway, runtime-host list/decide/open-output (grant memory-only, real bounded chunks, four process event kinds, durable session vocabulary + migration 0008).
2. Renderer **primitives** (isolated): `CommandActivityRow`, `CommandApprovalBar`, `TerminalOutputView`, `GitChangesView`.
3. TS **gateway contracts**: `execGateway` / `tauriExecGateway` (and Git twins) with strict normalizers; native Tauri product path still RED.

**What is missing (the D3B seams):**

1. Desktop `ConversationService` has **no** process list/decide/open-output service and does not call `recover_process_proposals`.
2. `exec_commands.rs` is a RED test stub only; commands are **not** registered in `lib.rs`.
3. Desktop event pipeline `kind_str` does not map the four process protocol kinds (non-exhaustive vs agent-protocol).
4. TS `RuntimeEventKind` / `tauriConversationGateway` omit process kinds.
5. UI primitives are **not mounted** in `Workbench` / timeline / resource pane.
6. Dual-domain approval arbitration (write vs process) is undefined in product code.
7. Live output is **pull** (`open_process_output`); there is no chunk stream event — UI must re-open on process events + Review.

Controller integration and acceptance remain separate. D3AR2 closeout may still be in flight; D3B workers must treat runtime-host/process authority as read-only dependency and not re-open process policy.

---

## 1. Current source-to-render data flow and missing seams

### 1.1 Intended end-to-end flow (C4 design)

```text
Provider proposes process.run@1 {program, argv, cwd?}
        │
        ▼
RuntimeHost.execute_governed_process
  - inject host project / conversation / run / call
  - seal via GovernedProcessToolGateway
  - reserve durable pending_approval
  - emit receipt payloads: requested → (decided) → (started) → terminal
        │
        ├─ durable: SQLite exec_proposals (metadata/digests only)
        ├─ durable: run_events process kinds (0008 CHECK)
        └─ live: memory grant + bounded stdout/stderr chunks
        │
        ▼
ConversationService pipeline
  - map protocol RuntimeEvent → domain RuntimeEventKind
  - persist RunEvent; emit conversation://runtime-event
        │
        ▼
TS ConversationGateway.subscribe
  - bump process re-list token (like write)
  - reducer projects activity summary into timeline
        │
        ├─ pending → single CommandApprovalBar above composer
        │             decideExecProposal({conversationId, proposalId, decision})
        │             (no grant in renderer — host private)
        ├─ activity → CommandActivityRow in conversation disclosure
        └─ review/output → ResourcePane Terminal mode
                          openExecOutput({conversationId, proposalId})
                          live_retained=false after restart → durable-only notice
```

### 1.2 Evidence of current layers

| Layer | Path | Status |
| --- | --- | --- |
| Protocol events + payloads | `crates/agent-protocol/src/types.rs` (`ProcessProposal*`, `ProcessStarted`, `ProcessTerminalOutcome`) | Present; deny-unknown; no grant/native/transcript |
| Session vocabulary | `crates/kernel-session/src/domain.rs` (21 kinds incl. four process) | Present (D3AR2) |
| SQLite CHECK | `crates/storage-sqlite/migrations/0008_run_events_process_kinds.sql` | Present (D3AR2) |
| Process gateway + live output | `crates/tool-executor/src/process.rs`, `crates/process-executor/src/output.rs` | Present |
| Runtime-host public API | `RuntimeHost::{list,decide,open}_process_*` in `crates/runtime-host/src/host/mod.rs` | Present post-D3AR |
| Desktop pipeline map | `apps/desktop/src-tauri/src/conversation_service/pipeline.rs` `kind_str` | **Missing process arms** |
| Desktop write pattern | `write_commands.rs` + `write_service.rs` + `WriteApprovalBar` | Complete for writes |
| Desktop process commands | `exec_commands.rs` | **RED stub only** (`is_exec_public_error` test) |
| Command registration | `apps/desktop/src-tauri/src/lib.rs` `generate_handler!` | write only; **no list_exec_*** |
| Process recovery on boot | `ConversationService::recover_inflight` | recovers writes only; **no process recover** |
| TS exec DTOs | `apps/desktop/src/lib/execGateway.ts`, `tauriExecGateway.ts` | Present; invoke names ready |
| TS process event kinds | `conversationGateway.ts` `RuntimeEventKind` | **write only** |
| Isolated UI primitives | `apps/desktop/src/features/execution/*` | Present; unit-tested |
| Workbench mount | `Workbench.tsx` | write bar only; **no CommandApprovalBar / TerminalOutputView** |
| App re-list token | `App.tsx` `writeProposalRefreshToken` | write events only |

### 1.3 C3 write path (working pattern to mirror, with C4 differences)

Working C3 flow:

1. Host emits `write_proposal_requested` / `write_proposal_resolved`.
2. Desktop pipeline maps kinds → SQLite → Tauri event.
3. App bumps `writeProposalRefreshToken`.
4. Workbench `listWriteProposals(conversationId)` under project+conversation generation guard.
5. `WriteApprovalBar` holds raw `approvalGrant` in a ref (not React state objects painted to DOM).
6. Review opens host-built diff in resource pane.

**C4 process must differ on grants:** runtime-host exposes `decision_available: bool` only; `decide_process_proposal(conversation, proposal, decision)` accepts **no grant argument**. Do not copy C3 grant-through-renderer for process.

### 1.4 Missing seams (ordered by blocker severity)

| # | Seam | Why it blocks D3B |
| --- | --- | --- |
| S1 | Native `list_exec_proposals` / `decide_exec_proposal` / `open_exec_output` | TS gateway invokes these; they do not exist as product commands |
| S2 | `conversation_service/exec_service.rs` (or equivalent) mapping host views → camelCase DTOs | Second-authority risk if Tauri calls tool-executor directly |
| S3 | Register commands + window conversation capability check (mirror write) | Ownership boundary at desktop shell |
| S4 | Pipeline `kind_str` + status_for_kind handling for four process kinds | Events cannot enter durable timeline / live stream |
| S5 | Adapter/host **caller-emits** wiring into conversation pump | Receipts exist; conversation may not yet emit/persist them on real turns |
| S6 | `recover_process_proposals` in `recover_inflight` | Restart law: non-terminal → interrupted |
| S7 | TS event parse + re-list token for process kinds | Bar and activity stay stale without poll (forbidden) |
| S8 | Mount primitives + dual-domain single-bar arbitration | Product UX incomplete |
| S9 | Terminal pane mode host (open output, generation discard, restart notice) | Progressive disclosure L2 |
| S10 | Trusted Git composition (out of process UI critical path but same D3 round) | Blocks Git Changes product path; not required for process bar |

### 1.5 Live output model (must not be mis-designed in UI)

From D3AR / D3AR2 host behavior:

- Chunks are host-memory; SQLite stores digests/metadata only.
- `open_process_output` returns live chunks while retained; after restart or eviction → metadata + empty chunks + `live_retained=false`.
- Caps: 256 KiB combined, 4,000 display lines; control neutralized before projection.
- **No** unbounded durable run_event for stdout/stderr text.
- UI must show explicit “live output not retained” after restart; never invent transcript from digests.

Recommended UI refresh (no polling interval):

1. On any of the four process runtime events for selected conversation → re-list proposals.
2. If Terminal mode open for that proposal → re-call `openExecOutput`.
3. Optional later: native push event for output sequence; **not** required for C4.0 if pull-on-event is proven.

---

## 2. Proposed typed DTOs and command/event names

### 2.1 Ownership boundaries

| Object | Owned by | Visible to renderer |
| --- | --- | --- |
| Project root / capability | Host + selected project | Never native root |
| Conversation | Project-scoped SQLite | `conversationId` as opaque key for invokes only |
| Run | Conversation | Internal `runId` for stream correlation; **never paint** |
| Proposal | Conversation + run + tool call | `proposalId` for invokes only; never paint in DOM text |
| Grant | Host live memory | **Not on wire** for process; only `decisionAvailable` |
| Live output | Host memory buffer | Sanitized chunks via open-output |
| Durable receipt | SQLite exec_proposals + process run_events | Summary fields only |

**Hard rules:** project switch hard-clears process projection; conversation switch parks it; neither moves ownership nor auto-cancels foreign process.

### 2.2 Tauri commands (exact names — match existing TS)

| Command | Args | Returns | Authority check |
| --- | --- | --- | --- |
| `list_exec_proposals` | `{ conversationId }` | `ExecProposalView[]` (≤256) | Window conversation capability + project root bind |
| `decide_exec_proposal` | `{ conversationId, proposalId, decision }` | `ExecProposalView` | Same; decision ∈ `approve_once` \| `reject`; **no grant field** |
| `open_exec_output` | `{ conversationId, proposalId }` | `ExecOutputView` | Same; foreign → public error |

Map host errors to closed vocabulary in `execGateway.ts` (`process_ownership_mismatch`, `process_not_live`, `process_expired`, …). Never forward native paths, OS messages, or grant digests.

### 2.3 Wire DTOs (renderer-facing; camelCase)

Already defined in `execGateway.ts` — **lock as product contract:**

```ts
ExecProposalView = {
  proposalId, conversationId,
  origin: "agent" | "user",
  state: "pending_approval" | "approved" | "running" | "succeeded" | "failed"
       | "rejected" | "expired" | "cancelled" | "interrupted",
  programLabel, argv: string[], portableCwd,
  riskClass: "standard" | "high",
  summary, expiresAt,
  decisionAvailable: boolean,  // not approvalGrant
  errorCode?: ExecPublicError
}

ExecOutputView = {
  proposalId, state,
  chunks: { stream: "stdout"|"stderr", sequence: number, text: string }[],
  truncated, encodingLossy, capturedBytes,
  exitCode: number | null, durationMs: number | null
}
```

Native serde should emit camelCase matching write DTOs. Host `LiveProcessProposalView` fields map as:

| Host | Renderer |
| --- | --- |
| `program` | `programLabel` |
| `portable_cwd` | `portableCwd` (default `"."`) |
| `decision_available` | `decisionAvailable` |
| `risk_class` | `riskClass` |
| states | snake_case enum strings identical to policy |

**Do not** expose host `live_retained` as authority; if needed for restart UX, add explicit optional `liveRetained?: boolean` only after normalizer update + tests (or derive from empty chunks + terminal state + notice string in parent).

### 2.4 Runtime events (snake_case kinds — already in protocol/session)

| Kind | Payload (summary-only) | UI effect |
| --- | --- | --- |
| `process_proposal_requested` | proposal_id, program, argv, portable_cwd?, summary, risk_class, expires_at | Re-list; show bar if pending + selected conversation |
| `process_proposal_decided` | proposal_id, program, decision, state | Re-list; clear bar if no longer pending |
| `process_started` | proposal_id, program, argv, portable_cwd? | Activity → running; optional open-output refresh |
| `process_terminal_outcome` | proposal_id, program, state, exit_code?, duration_ms?, truncated?, encoding_lossy?, captured_bytes?, cancelled?, timed_out? | Activity collapses; re-list; output open may become durable-only |

Order on approve path: **requested → decided → started → terminal**. Reject/expire/cancel/interrupt: **no started**. Busy/replay: no second start/spawn.

### 2.5 UI-local view models (already in `executionTypes.ts`)

| View | Source | Role |
| --- | --- | --- |
| `CommandProposalView` | subset of `ExecProposalView` | Approval bar (no decisionAvailable field on proposal object; parent passes separately) |
| `CommandActivityView` | terminal event + optional tail | Timeline progressive disclosure L0/L1 |
| `TerminalOutputModel` | `ExecOutputView` + program/argv labels | Resource pane L2 |

Projection rule: parent maps gateway → UI models; primitives stay pure (no Tauri).

### 2.6 Dual-domain approval arbitration (required product rule)

Exactly **one** bar above the composer for the selected conversation:

```text
candidates = pending_write ∪ pending_process
selected   = oldest by expiresAt ascending, then proposalId ascending
if both domains present and ages tie: prefer write (stable; C3 first)
never render two bars
```

Document this in Task 11 tests. Process bar uses `CommandApprovalBar` (`decisionAvailable`); write bar remains `WriteApprovalBar` (grant ref). Parent switches component by domain of selected candidate.

---

## 3. File-level implementation slices (three+ non-overlapping workers)

Global constraints for all slices: no stage/commit/reset/restore/clean/package/publish; preserve HEAD and unrelated dirty work; do not broaden process catalog, add PTY/shell, second executor, grants on process wire, or transcript persistence.

### Worker A — Native process bridge (desktop authority surface)

**Goal:** Product Tauri commands over **only** `ConversationService` → `RuntimeHost` process APIs.

| Allow | Deny |
| --- | --- |
| `apps/desktop/src-tauri/src/exec_commands.rs` | `crates/tool-executor/**` |
| `apps/desktop/src-tauri/src/conversation_service/exec_service.rs` (create) | `crates/process-executor/**` |
| `apps/desktop/src-tauri/src/conversation_service/mod.rs` | `crates/runtime-host/**` (read-only dependency) |
| `apps/desktop/src-tauri/src/conversation_service/service.rs` (recover + thin wrappers only) | Git native |
| `apps/desktop/src-tauri/src/lib.rs` (register three commands only) | Renderer UI |
| `apps/desktop/src-tauri/src/conversation_service/pipeline.rs` (process kind map arms) | |
| focused Rust tests under desktop / conversation_service | |

**Deliverables:** GREEN `list`/`decide`/`open` with window capability; error mapping; startup `recover_process_proposals`; pipeline accepts four process kinds into durable events.

**Note on common entry:** `lib.rs` and `service.rs` are exclusive to Worker A for the process command surface. Worker C must not edit them.

### Worker B — TS transport + projection (events and gateways)

**Goal:** Accept process events end-to-end in renderer gateway and activity projection; keep strict DTO denial tests green.

| Allow | Deny |
| --- | --- |
| `apps/desktop/src/lib/conversationGateway.ts` | `Workbench.tsx`, `App.tsx` mount logic |
| `apps/desktop/src/lib/tauriConversationGateway.ts` | Native Rust |
| `apps/desktop/src/lib/execGateway.ts` (only if normalizer gaps) | `features/execution/*` visual polish |
| `apps/desktop/src/lib/tauriExecGateway.ts` | Git unless fixing shared test only |
| `apps/desktop/src/features/conversations/agentActivityFromTimeline.ts` | |
| `apps/desktop/src/features/conversations/agentActivityTypes.ts` | |
| `apps/desktop/src/features/conversations/agentActivityProjection.ts` | |
| `apps/desktop/src/features/conversations/conversationReducer.ts` (if event reduce needed) | |
| `apps/desktop/tests/c4-gateway-contract.test.ts` | |
| `apps/desktop/tests/agent-activity-*.test.tsx` (process outcomes) | |
| `apps/desktop/tests/conversation-gateway-stream.test.tsx` (process kinds) | |

**Deliverables:** four process kinds in `RuntimeEventKind` + payload types; strict parse; process outcomes in agent activity (path-free, no raw IDs in DOM); gateway contract still rejects grant/native/unknown fields.

**DTO alignment fix:** ensure `normalizeExecProposal` fixture/tests always require `decisionAvailable` (present in production normalizer).

### Worker C — Workbench progressive disclosure mount

**Goal:** Conversation-centred product UX only.

| Allow | Deny |
| --- | --- |
| `apps/desktop/src/App.tsx` (process re-list token; wire gateway) | `src-tauri/**` |
| `apps/desktop/src/main.tsx` (construct `TauriExecGateway`) | Protocol/policy crates |
| `apps/desktop/src/features/workbench/Workbench.tsx` | Expanding process authority |
| `apps/desktop/src/features/workbench/ResourcePane.tsx` | |
| `apps/desktop/src/features/conversations/RunActivity.tsx` and/or `ConversationTimeline.tsx` | |
| `apps/desktop/src/features/conversations/AgentActivityDisclosure.tsx` (if needed) | |
| `apps/desktop/src/features/execution/*` (mount hooks only; primitives already exist) | |
| `apps/desktop/src/styles/execution.css`, workbench CSS | |
| `apps/desktop/tests/c4-workbench-integration.test.tsx` (create) | |
| `apps/desktop/tests/c4-execution-primitives.test.tsx` (regressions only) | |

**Deliverables:**

1. Summary row in conversation activity (`CommandActivityRow`).
2. Single approval bar with dual-domain arbitration.
3. Review / 完整输出 opens Terminal mode in contextual pane (`TerminalOutputView`).
4. Project switch hard-clear; conversation switch generation discard; Esc reject when bar owns focus; no second dashboard; no raw session/run IDs in DOM.

### Worker D (optional later / Task 12) — Desktop E2E

| Allow | Deny |
| --- | --- |
| `apps/desktop/tests/e2e/c4-governed-process*.spec.ts` | Product logic except harness-only waits |
| `scripts/wave-c4-*-acceptance.mjs` | Authority changes |
| screenshots under `artifacts/` | |

Depends on A+B+C green. Controller owns acceptance docs.

### Slice dependency order

```text
A (native + pipeline) ──► B (TS events) ──► C (mount)
                └── parallel after A exports stable DTO shapes ──►
Controller serial review after each; common files already partitioned.
```

If D3AR2 integration gate is still open, **pause A** until controller declares runtime-host process surface accepted; B/C can still prepare tests against mocks.

---

## 4. Test matrix

### 4.1 Rust command / service tests

| Case | Proof |
| --- | --- |
| List only own conversation | Foreign conversation → `process_ownership_mismatch` / not_live |
| Window capability required | Unbound window cannot list/decide |
| Decide approve_once once | Second decide fails closed; no second spawn |
| Decide reject | Terminal rejected; no start event |
| Expiry | `process_expired`; decisionAvailable false |
| Open output live | Ordered chunks; byte/line caps |
| Open output foreign | Denied |
| Open after restart | Empty chunks; durable metadata; no transcript invent |
| Recover inflight | Non-terminal → interrupted; residual authority 0 |
| Pipeline map_kind | All four process kinds parse + persist |
| Public error enum | Unknown codes rejected (`c4_gateway` style) |
| No grant in Debug/serialize | Snapshot/log privacy |

Commands (examples):

```text
cargo test -p pillow-desktop --lib c4_gateway -- --nocapture
cargo test -p pillow-desktop --test project_commands  # if shared
cargo test -p keeper-runtime-host --test governed_process_bridge
```

### 4.2 TS gateway tests

| Case | File target |
| --- | --- |
| Reject unknown fields, native paths, grant, runId | `c4-gateway-contract.test.ts` |
| Reject invalid enums / oversized output | same |
| Normalize happy path with decisionAvailable | same (add if missing) |
| Parse process_* events; reject secret keys in payload | `tauriConversationGateway` / stream tests |
| normalizeExecError maps closed codes | unit |

```text
corepack pnpm --filter @pillow/desktop test -- c4-gateway-contract.test.ts
corepack pnpm --filter @pillow/desktop typecheck
```

### 4.3 React interaction tests

| Case | Proof |
| --- | --- |
| CommandActivityRow progressive disclosure | summary → tail → open output callback |
| CommandApprovalBar approve/reject/review | Esc reject; Tab trap; focus primary |
| High risk note for node -e / python -c class | riskClass high copy |
| Dual-domain single bar | never two bars; oldest selection |
| TerminalOutputView order by sequence | stdout/stderr display |
| Agent activity process outcomes | fixed labels; no raw IDs |
| 1024 layout class / no permanent card stack | workbench integration |

```text
corepack pnpm --filter @pillow/desktop test -- c4-execution-primitives.test.tsx c4-workbench-integration.test.tsx
```

### 4.4 Project-switch isolation

| Case | Proof |
| --- | --- |
| Switch project | Clear pending bar, terminal target, grant-equivalent decision memory, generation bump |
| Switch conversation | Park other conversation’s bar; no cross decide |
| Stale async list | Generation guard discards late results |
| Foreign proposal id | Decide/open fail closed |

### 4.5 Restart / durable-only output

| Case | Proof |
| --- | --- |
| Restart mid-pending | interrupted; no auto resume |
| Restart mid-running | interrupted; cleanup proven at host layer |
| Reopen terminal after restart | notice: live output not retained; digests/metadata only |
| Terminal replay identity | Replayed delivery without second spawn (host); UI shows terminal state |

### 4.6 Desktop E2E (Task 12)

Scenarios from C4 design §10 / plan Task 12: reject, approve-success, nonzero-exit, expiry, cancel, timeout, truncation/lossy, conversation isolation, forced restart residual process proof, screenshots 1024×720 and 1440×960 with conversation dominant and one bar.

```text
corepack pnpm --filter @pillow/desktop build
corepack pnpm desktop:test:e2e   # serial C4 suite when added
```

No test may introduce numeric PID kill, name-wide cleanup, or grant logging.

---

## 5. UI interaction model

### 5.1 Progressive disclosure ladder

```text
L0  Conversation timeline summary row
    origin · status · programLabel · argv display · exit · duration
        │ expand <details>
L1  Nested tail (bounded lines) + truncation/lossy notices + “完整输出”
        │ open contextual pane
L2  Terminal output mode (not a shell)
    sanitized chunks, byte count, exit, duration
        │ structured tests / files (if produced via existing channels)
L3  Existing resource / artifact / write-diff modes
```

**Never:** global terminal dashboard, permanent successful-command card stack, debug tables of session/run/proposal IDs, second approval bar, interactive PTY.

### 5.2 Placement (reuse accepted chrome)

| Surface | Placement |
| --- | --- |
| Activity summary | Inside owning conversation (AgentActivity / RunActivity / adjacent CommandActivityRow) |
| Approval bar | Single slot above composer (C3 position) |
| Terminal output | Mode of collapsible right resource pane at 1440×960; may use stage-bottom strip ≤40% height at 1024×720 |
| Git Changes | Same pane, separate mode (G0; Worker C can stub mode switch without Git native if blocked) |

### 5.3 Keyboard / focus

| Key | Behavior |
| --- | --- |
| Focus enter bar | Primary **批准一次** focused |
| Tab / Shift+Tab | Cycle Approve · Reject · Review within bar |
| Escape | Reject **only when bar owns focus** |
| Closing pane | Does **not** cancel process |
| Cancel run | Explicit existing cancel path; host process cancel ≤ one terminal receipt |

### 5.4 Origin and risk display

- Origin labels: 代理 / 用户 (C4.0 user origin only if host quick-actions exist later).
- Risk: 标准 / 高风险; high-risk note: project path containment is **not** an OS sandbox.
- Argv display: `formatArgvDisplay` (JSON.stringify per arg) — display only, never re-parse for execution.

### 5.5 Reference patterns (conceptual only — do not copy code)

| Source | Pin | License | Adopt |
| --- | --- | --- | --- |
| Grok App | `cbbbdf17bb8db1588b6707f3f42f6b00397e27f9` at `<workspace>\VaultSystem\references\grok-app` | MIT | Conversation-dominant layout; single permission bar; tool-in-timeline; Changes in resource pane; multi-session park |
| zbbody / AionUi | `00c293414333b6fcc4e4598389fa3952c8d0b09b` at `<workspace>\zbbody` | Apache-2.0 | Command progressive disclosure; collapsed group summary; turn fold |
| Pillow C3 write bar | dirty tree | Pillow | Generation guards, Esc reject, grant privacy (process: decisionAvailable instead) |

**Provenance rule:** conceptual reuse only for Grok App / zbbody. Any adapted code requires `THIRD_PARTY_NOTICES.md` update. Grok App has **no** interactive PTY panel at pin — shell is a tool step; Pillow designs Terminal **output** mode deliberately.

---

## 6. Risks and explicit out-of-scope

### 6.1 Risks

| Risk | Mitigation |
| --- | --- |
| Second authority via Tauri → tool-executor bypass | Only `RuntimeHost` through `ConversationService` |
| Copying C3 grant into process renderer | Process decide has no grant; tests reject `approvalGrant` on ExecProposalView |
| Dual bars / race with writes | Deterministic oldest-pending arbitration + tests |
| Pipeline non-exhaustive match vs process kinds | Worker A adds four arms before shipping events |
| Live output race after waiter consume | Host terminal cache (D3AR2); UI re-open on events; accept empty + notice |
| Concurrent D3AR2 controller gate | D3B prep may land docs/mocks; native mount waits for accepted host surface |
| Painting internal IDs | Forbidden display id lists (agent activity pattern) |
| Polling proposals | Event-driven tokens only |
| Git trust PATH guess | Out of process UI critical path; controller decides trusted git composition separately |
| Workspace fmt/clippy pre-existing noise | Package-scoped gates; do not “fix” unrelated trees |

### 6.2 Explicit out of scope (D3B / C4.0)

- Broadening process catalog (`git`, shells, script shims, arbitrary executables)
- Second process executor or provider-native process tools
- Exposing or persisting grants
- Full transcript persistence or reconstructing output from digests
- Interactive ConPTY / free-form shell (C4.1 design only)
- Git mutation (C4.2 design only)
- Network tools, credentials, Obsidian, company ops, multi-runtime expansion
- Permanent task/process dashboard
- Numeric PID / taskkill / name-wide process cleanup

### 6.3 Controller decisions still open

1. Accept D3AR/D3AR2 runtime-host authority surface for product mount.
2. Trusted `git.exe` + generation owner composition for desktop (Git G0).
3. Whether adapter turn path already emits process receipt events into the conversation pump or needs a thin host emit slice (still not a second authority).
4. Final dual-domain bar tie-break if product wants process-before-write (default above prefers write).

---

## Key recommendations (for controller)

1. **Ship D3B as three serial-ish slices A→B→C** with the allowlists above; do not reopen process policy/executor in UI workers.
2. **Treat process grants as host-private** end-to-end; mirror C3 placement, not C3 grant transport.
3. **Use pull-on-event for live output** for C4.0; document restart durable-only UX explicitly in Terminal header.
4. **One bar, oldest pending across write∪process** with tests that fail on dual mount.
5. **Pause product native mount** if D3AR2 controller gate is still red; keep this map and mock-based UI work valid either way.

---

## References (in-tree)

- Design: `docs/superpowers/specs/2026-08-02-c4-governed-process-git-design.md`
- Plan: `docs/superpowers/plans/2026-08-02-c4-governed-process-git-implementation.md` (Tasks 10–12)
- Prior research: `docs/research/2026-08-02-c4-terminal-git-interaction-research.md`, `docs/research/2026-08-01-c3-grok-app-interaction-research.md`
- D3A/D3AR evidence: `docs/execution/2026-08-04-wave-c4-d3a-*`, `docs/execution/2026-08-04-wave-c4-d3ar*`, `docs/verification/2026-08-04-wave-c4-d3a-controller-review.md`
- Upstream baseline: `docs/references/upstream-baseline.md`

---

## Completion

- HEAD preserved: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
- Staging empty
- Only this allowlisted report written
- No product code, stage, commit, reset, restore, clean, package, or publish

**COMPLETION: complete**
