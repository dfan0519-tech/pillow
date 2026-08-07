# C4 D3B2 Workbench Mounting Implementation Plan

> **For agentic workers:** Execute only the assigned task and exact write
> allowlist. Do not stage, commit, reset, restore, clean, package, or publish.

**Goal:** Mount the accepted governed-process contracts into Pillow's existing
conversation-centred workbench with one approval slot, progressive disclosure,
bounded terminal output, strict project/conversation isolation, and a real
Tauri acceptance path.

**Architecture:** `App` converts accepted process lifecycle events into an
event-driven refresh token. `Workbench` lists conversation-owned process
proposals through `ExecGateway`, arbitrates them with write approvals, and
opens one terminal-output target in the existing contextual pane. The pane
pulls bounded host output only on explicit review and accepted process events;
it never polls or persists transcripts.

**Tech Stack:** React 19, TypeScript, Vitest/testing-library, Tauri 2, Rust,
SQLite-backed conversation authority, real WebView2 E2E.

## Global Constraints

- Only Stage 1 C4.0 is active; no PTY, shell, Git mutation, Obsidian, key
  management, network tools, or runtime expansion.
- Process grants stay host-private. Renderer decisions carry only
  `conversationId`, `proposalId`, and `approve_once | reject`.
- Never render native paths, project roots, run/session/proposal IDs, debug
  metadata, environment values, or raw output outside `TerminalOutputView`.
- One approval-bar slot exists above the composer across write and process.
- Refresh is event-driven. No interval polling or permanent process dashboard.
- Project switch hard-clears process surfaces. Conversation switch parks the
  foreign proposal and rejects stale async commits by generation.
- The controller alone integrates, stages/commits, and runs aggregate gates.

---

### Task 1: Preserve durable-only output truth on the typed bridge

**Owner:** Codex2 account 2, `gpt-5.6-terra`, high, `fastMode=false`.

**Files:**

- Modify: `apps/desktop/src-tauri/src/conversation_service/exec_service.rs`
- Modify: `apps/desktop/src-tauri/src/exec_commands_tests.rs`
- Modify: `apps/desktop/src/lib/execGateway.ts`
- Modify: `apps/desktop/src/lib/tauriExecGateway.ts`
- Modify: `apps/desktop/tests/c4-gateway-contract.test.ts`
- Modify: `apps/desktop/tests/tauri-conversation-gateway.test.ts` only if the
  existing shared fixture requires parity
- Create: `docs/execution/2026-08-04-wave-c4-d3b2-codex2-live-retained-report.md`

**Produces:** `ExecOutputView.liveRetained: boolean`, copied from the
host-owned `ProcessOutputView.live_retained`. The strict normalizer requires
the exact camelCase key and rejects wrong types, unknown authority fields, and
missing truth.

- [ ] Add a failing Rust serialization test proving `liveRetained` is present
  while grants, native paths, executable identity, and transcript persistence
  fields remain absent.
- [ ] Add failing TypeScript normalization tests for true, false, missing,
  wrong-type, and extra-field cases.
- [ ] Add the one field to native and TS DTOs without changing process
  authority or output caps.
- [ ] Run focused Rust tests, focused gateway tests, and typecheck.
- [ ] Write the completion report with changed files, tests, risks, and
  `COMPLETION: complete|blocked`.

### Task 2: Wire event-driven process refresh into App composition

**Owner:** Grok4 registered slot.

**Files:**

- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/main.tsx`
- Create: `apps/desktop/tests/c4-app-process-refresh.test.tsx`
- Create: `docs/execution/2026-08-04-wave-c4-d3b2-grok4-app-refresh-report.md`

**Consumes:** Existing `ExecGateway` and `TauriExecGateway`.

**Produces:** Optional `App.execGateway`, one
`execProposalRefreshToken`, and exact `Workbench` props named `execGateway`
and `execProposalRefreshToken`.

- [ ] Add failing tests proving only the selected, active-project conversation
  bumps the token for the four accepted process kinds.
- [ ] Prove write-event refresh remains unchanged and foreign project or
  conversation events cannot refresh the selected surface.
- [ ] Construct one `TauriExecGateway` in `main.tsx` and pass it through App;
  do not add polling, grants, or another global store.
- [ ] Run the focused test and desktop typecheck. If Workbench props are not
  present yet, record the exact integration dependency instead of editing
  Workbench outside the allowlist.
- [ ] Write the completion report and send the completion signal.

### Task 3: Mount one approval slot and terminal-output contextual mode

**Owner:** Grok4 registered slot after Tasks 1-2 controller acceptance.

**Files:**

- Modify: `apps/desktop/src/features/workbench/Workbench.tsx`
- Modify: `apps/desktop/src/features/workbench/ResourcePane.tsx`
- Modify: `apps/desktop/src/features/execution/executionTypes.ts`
- Modify: `apps/desktop/src/features/execution/TerminalOutputView.tsx`
- Modify: `apps/desktop/src/styles/workbench.css` only for layout integration
- Modify: `apps/desktop/src/styles/execution.css` only for missing mounted-state
  styling
- Create: `apps/desktop/tests/c4-workbench-process-integration.test.tsx`
- Create: `docs/execution/2026-08-04-wave-c4-d3b2-grok4-workbench-mount-report.md`

**Consumes:** `execGateway?: ExecGateway` and
`execProposalRefreshToken?: number`; accepted `selectVisibleApproval`;
`ExecOutputView.liveRetained` from Task 1.

**Produces:** One selected write-or-process approval bar, process
list/decide/review state under generation guards, and a `Terminal output` mode
inside the existing resource pane.

- [ ] Add failing tests for write-only, process-only, simultaneous candidates,
  exact ties, expired/unavailable candidates, and never rendering two bars.
- [ ] Add failing tests for approve-once/reject with no grant and native
  authoritative re-list after decision.
- [ ] Add failing project/conversation switch tests that resolve stale list and
  output promises after the switch and prove no foreign bar, output, or error
  paints.
- [ ] Mount `CommandApprovalBar`; preserve write grants only in the existing
  ref map and keep process authority out of state/DOM/logs.
- [ ] Mount `TerminalOutputView`; refresh only when its owning process event
  token changes, show an explicit durable-only notice when
  `liveRetained=false`, and never reconstruct transcript from metadata.
- [ ] Prove Esc rejects only the focused process bar; closing the pane does not
  cancel a process; 1024 layout keeps conversation/composer dominant.
- [ ] Run focused tests, execution primitives regression, and typecheck.

### Task 4: Mount compact command activity progressive disclosure

**Owner:** Grok2 registered slot after Task 3 controller acceptance.

**Files:**

- Create: `apps/desktop/src/features/execution/processActivityModel.ts`
- Modify: `apps/desktop/src/features/workbench/Workbench.tsx`
- Modify: `apps/desktop/src/features/conversations/ConversationTimeline.tsx`
  only if the accepted Workbench composition cannot keep rows adjacent to the
  owning timeline without changing timeline props
- Create: `apps/desktop/tests/c4-process-activity-integration.test.tsx`
- Create: `docs/execution/2026-08-04-wave-c4-d3b2-grok2-process-activity-report.md`

**Produces:** A `CommandActivityRow` owned by the conversation, with safe
program label, literal argv display, terminal metadata, optional bounded tail,
and `完整输出` routing to the existing terminal pane. Internal IDs never paint.

- [ ] Project from normalized process data without duplicating authority.
- [ ] Keep proposal/decision/start/terminal summary-first ordering.
- [ ] Prove raw IDs, native paths, debug data, grants, and output transcript do
  not appear in activity DOM.
- [ ] Prove successful commands collapse concisely and do not form a permanent
  card stack.

### Task 5: Real Tauri process-governance E2E

**Owner:** Codex2 account 2 for harness implementation; controller for final
execution and verdict.

**Files:**

- Create: `scripts/wave-c4-d3b-process-acceptance.mjs`
- Create: `apps/desktop/tests/e2e/c4-d3b-process-governance.spec.ts`
- Modify: `apps/desktop/package.json`
- Create: `docs/execution/2026-08-04-wave-c4-d3b2-codex2-e2e-report.md`
- Controller creates:
  `docs/verification/2026-08-04-wave-c4-d3b2-acceptance.md`

- [ ] Prove a real provider-origin proposal appears in exactly one selected
  conversation and one approval bar.
- [ ] Prove reject emits no start; approve-once emits one start and one terminal
  outcome; replay does not spawn twice.
- [ ] Prove bounded stdout/stderr, truncation/lossy metadata, and output-pane
  reopening.
- [ ] Prove project/conversation switching cannot decide or display foreign
  process state.
- [ ] Prove restart marks non-terminal authority interrupted and shows
  durable-only output truth.
- [ ] Prove normal close leaves zero owned descendants.

## Controller Aggregate Gate

After every worker completion signal, the controller reviews the exact diff
before running:

```text
pnpm --filter @pillow/desktop typecheck
pnpm --filter @pillow/desktop exec vitest run <focused D3B files>
cargo test -p pillow-desktop exec_commands_tests -- --nocapture
pnpm check
pnpm --filter @pillow/desktop build
pnpm desktop:test:e2e
cargo fmt --all -- --check
git diff --check
```

The controller also verifies HEAD/staging, worker allowlists, no unowned
processes, and zero relevant residual processes. D3B is not complete until the
new real process-governance E2E passes; Tasks 1-4 alone are not a checkpoint.
