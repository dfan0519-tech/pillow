# C4 D3B3 Real Desktop Process Acceptance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove the accepted provider-origin governed process path through the real Tauri/WebView2 desktop UI, including approval, conversation activity, terminal output, isolation, replay, cancellation, restart, visual, and cleanup boundaries.

**Architecture:** Reuse the production Grok ACP composition with `fake_acp_agent` as the executable, the existing C3 isolated app-data/WebView2/CDP harness, and `scripts/lib/desktop-process.mjs` ownership-based supervision. Add a C4-only interaction helper and orchestrator; production React, Tauri commands, runtime-host, policy, and executor code remain read-only. Each scenario enters through the public composer and production adapter, never by invoking an executor or injecting renderer state.

**Tech Stack:** Node test runner, Playwright Core over WebView2 CDP, Tauri 2, React, Rust fake ACP fixture, Windows process supervision.

## Global Constraints

- Work only in `<repo>`; Keeper is record-only and cannot supply prompts, context, code, or acceptance decisions.
- Preserve the dirty tree. Do not stage, commit, reset, restore, clean, or format unrelated files.
- Do not modify production UI, Tauri commands, runtime-host, policy, storage, or process executor. If a product defect is found, write a reproducible blocker in the report and stop.
- Never kill by numeric PID or process name. Cleanup must use the existing launcher-owned `ProcessSupervisor` and immutable command-line/profile ownership evidence.
- Use production Grok ACP composition with the repository-built fake ACP binary. Do not use `PILLOW_RUNTIME=fake` and do not bypass `ConversationService` or `RuntimeHost`.
- Use deterministic locator/state waits. Do not turn failures green by increasing fixed sleeps or weakening assertions.
- Screenshots must redact native paths, UUID/session/run/proposal identifiers, credentials, and raw provider envelopes before capture.
- Controller alone writes C4 acceptance/checkpoint documents and runs the aggregate gate.

---

### Task 1: C4 WebView interaction helper

**Files:**
- Create: `scripts/lib/c4-governed-process-acceptance.mjs`

**Interfaces:**
- Consumes: Playwright `page` connected to the real Pillow WebView2 page.
- Produces: deterministic helpers for composer send, process approval/rejection, conversation activity, terminal output, isolation assertions, privacy checks, and screenshots.

- [ ] **Step 1: Implement public-surface locators**

Use only stable production selectors already present in the UI:

```js
export const PROCESS_TEST_IDS = Object.freeze({
  approval: "command-approval-bar",
  approve: "command-approval-approve",
  reject: "command-approval-reject",
  review: "command-approval-review",
  activity: "command-activity-row",
  activityDetails: "command-activity-details",
  activityStatus: "command-activity-status",
  activityOpenOutput: "command-activity-open-output",
  terminal: "terminal-output-view",
  terminalBody: "terminal-output-body",
});
```

Every wait must check visible state and the expected status/text. Reject DOM text containing drive-root paths, UUIDs, `sessionId`, `runId`, `proposalId`, `approvalGrant`, bearer/token/key markers, or raw JSON-RPC fields.

- [ ] **Step 2: Implement approval and activity helpers**

Provide named exports `sendProcessPrompt`, `waitForCommandApproval`, `approveCommandOnce`, `rejectCommand`, `waitForCommandActivity`, `openCommandOutput`, `assertSingleApprovalSlot`, `assertConversationDominant`, `assertNoForeignProcessEvidence`, and `capturePrivacySafeScreenshot`. `assertSingleApprovalSlot` must count the union of write and process approval bars and require at most one.

- [ ] **Step 3: Syntax-check the helper**

Run: `node --check scripts/lib/c4-governed-process-acceptance.mjs`

Expected: exit 0.

### Task 2: Deterministic provider process scenarios

**Files:**
- Modify: `crates/adapter-grok-acp/src/bin/fake_acp_agent/main.rs`
- Modify: `crates/adapter-grok-acp/src/bin/fake_acp_agent/scripts.rs`

**Interfaces:**
- Consumes: a normal provider prompt over production ACP transport.
- Produces: fixed process-tool requests for desktop scenarios, always using `pillow/process_tool` and portable `node` arguments.

- [ ] **Step 1: Extend the script catalog**

Keep the accepted approve/reject/replay/conflict scripts. Add only the scenarios the desktop orchestrator needs:

```text
governed-process-nonzero       node -e "process.stderr.write('expected-stderr');process.exit(7)"
governed-process-output-bounds node -e writes over the accepted cap and emits invalid UTF-8 bytes
governed-process-long-tree     node -e spawns one long-lived child and remains live until cancellation
governed-process-timeout       node -e remains live beyond the fixed governed timeout
```

Each provider script waits for the matching RPC response and emits a fixed terminal label. It must never treat raw output as provider success data or add a second executor path.

- [ ] **Step 2: Add focused adapter fixture coverage**

Use existing in-module tests or `governed_process_roundtrip.rs` only if essential and within the task allowlist. Prove exact method shape and that each new script produces one request and one provider completion. Do not weaken Task 5A replay/order tests.

- [ ] **Step 3: Run adapter gates**

Run:

```powershell
cargo test -p keeper-adapter-grok-acp --test governed_process_roundtrip -- --nocapture
cargo test -p keeper-adapter-grok-acp --all-targets -- --nocapture
```

Expected: all deterministic tests pass; only the explicit live-account test may remain ignored.

### Task 3: Real Tauri/WebView2 orchestrator

**Files:**
- Create: `scripts/wave-c4-governed-process-acceptance.mjs`

**Interfaces:**
- Consumes: repository-built debug Pillow and `fake_acp_agent`, isolated app data/Grok profile/WebView2 profile, generated CDP config.
- Produces: `.superpowers/c4-process-acceptance/c4-process-acceptance-record.json`, privacy-safe screenshots, and one terminal line prefixed `KEEPER_WAVE_C4_GOVERNED_PROCESS_ACCEPTANCE`.

- [ ] **Step 1: Reuse the accepted C3 launch envelope**

Adapt, do not import-and-mutate, the proven C3 orchestration boundaries: MSVC environment, isolated `CARGO_TARGET_DIR`, temporary app data, temporary Grok profile, temporary WebView2 profile, dynamic CDP port, production `PILLOW_GROK_EXECUTABLE`, sibling `keeper_fake_acp_script.txt`, `ProcessSupervisor`, normal-close proof, forced-restart cleanup proof, real-profile snapshot unchanged, and final residual/listener count zero.

- [ ] **Step 2: Implement core UI scenarios**

Drive these through the public UI and typed Tauri boundary:

```text
reject                 one bar; reject; requested/decided/terminal; no started
approve-success        approve once; requested/decided/started/terminal; exit 0
terminal-output        expand activity; open complete output in contextual pane
replay-conflict        exact replay has no second lifecycle/spawn; changed args fail closed
nonzero-exit           exit 7 is visible as terminal failure with bounded stderr
output-bounds          truncation and lossy notices are visible; bytes remain bounded
cancel-tree            cancel a running command; one terminal; child tree cleanup proven
timeout                fixed timeout terminal; no retry or second spawn
two-project-isolation  project B never shows A approval/activity/output; returning to A restores scope
conversation-switch    foreign conversation cannot decide or open A output
forced-restart         pending/running authority becomes interrupted; no auto-resume; residual process zero
```

Expiry is not simulated with a production force seam. If its fixed real duration is impractical, record expiry as covered by accepted lower-layer tests rather than weakening the product timeout.

- [ ] **Step 3: Capture the two required viewports**

At 1440x960 capture the single process approval bar and terminal output pane. At 1024x720 capture conversation-dominant activity with the contextual pane open. Require no second dashboard, no dual approval bars, no raw internal identifiers, and no native absolute path.

- [ ] **Step 4: Emit a strict terminal record**

The final JSON line must contain:

```js
{
  schemaVersion: 1,
  status: "passed",
  ipcBoundary: "real-tauri-webview",
  runtimePath: "production-grok-acp-with-fake-provider",
  scenarios: [/* exact successful scenario names */],
  viewports: ["1440x960", "1024x720"],
  processCleanup: "passed",
  normalClose: "passed",
  forcedRestartCleanup: "passed",
  residualProcesses: 0,
}
```

On any failure, emit `status:"failed"`, retain sanitized diagnostics, and exit nonzero.

### Task 4: Node E2E registration and worker report

**Files:**
- Create: `apps/desktop/tests/e2e/c4-governed-process.spec.ts`
- Modify: `apps/desktop/package.json`
- Create: `docs/execution/2026-08-05-wave-c4-d3b3-grok4-real-desktop-process-e2e-report.md`

**Interfaces:**
- The spec runs the orchestrator under `runSupervisedCommand` with serial ownership and parses the strict terminal record.
- `test:e2e` includes the new spec exactly once.

- [ ] **Step 1: Write the RED Node test**

Require `schemaVersion`, `status`, `ipcBoundary`, `runtimePath`, both viewports, cleanup fields, zero residual processes, and every implemented scenario name. Run before the orchestrator exists or before all scenarios pass and record the RED reason.

- [ ] **Step 2: Register the spec serially**

Append `tests/e2e/c4-governed-process.spec.ts` to the existing `test:e2e` script without changing the earlier suite order or concurrency.

- [ ] **Step 3: Run focused GREEN and static gates**

Run:

```powershell
pnpm --filter @pillow/desktop exec node --test --test-concurrency=1 tests/e2e/c4-governed-process.spec.ts
pnpm --filter @pillow/desktop typecheck
pnpm --filter @pillow/desktop build
cargo test -p keeper-adapter-grok-acp --all-targets -- --nocapture
cargo check --workspace --all-targets
cargo clippy -p keeper-adapter-grok-acp -p pillow-desktop --all-targets -- -D warnings
cargo fmt --all -- --check
```

Expected: all commands exit 0. Do not run the entire serial E2E suite unless the focused C4 test is green; the controller owns the aggregate run.

- [ ] **Step 4: Verify scope and report honestly**

Run `git diff --check --` against the exact allowlist. The report must list changed files, first RED, final GREEN counts and duration, scenario matrix, screenshot paths, cleanup evidence, any scenario deliberately deferred to lower-layer proof, residual risks, and `completion: complete|blocked`. Do not claim controller acceptance.

