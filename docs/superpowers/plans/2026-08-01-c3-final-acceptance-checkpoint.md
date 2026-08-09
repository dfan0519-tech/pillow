# Pillow C3 Final Acceptance And Checkpoint Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development for any conditional repair task. The Codex Desktop controller performs every acceptance gate personally. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a defensible C3 governed-write checkpoint from the current dirty working tree, without starting C4 or staging/committing any file.

**Outcome (2026-08-02):** C3 is controller-accepted and checkpointed in the
working tree. Grok4 and the real account-2 Codex CLI completed bounded
development; the controller closed all Important findings and passed the full
repository/build/desktop/hygiene gates. C4 remains planning-only.

**Architecture:** Treat the remaining work as a gated evidence pipeline rather than a new feature wave. First re-establish the process-safety invariant and focused real Tauri/WebView2 C3-F proof. Then route bounded development to Grok4 and Codex2 on non-overlapping allowlists, let the Codex Desktop controller review and rerun every affected gate, run the broad controller gates, and finally record the C3 acceptance/checkpoint documents.

**Tech Stack:** Rust workspace, Tauri 2, React 19, TypeScript, Vitest, Node test runner, Playwright CDP, Windows WebView2, SQLite, pnpm 10.14.0.

## Global Constraints

- Canonical repository: `this repository`.
- Frozen legacy repository: `legacy Keeper (out of tree)`; never write Pillow IDE functionality there.
- Baseline branch/HEAD: `main` / `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b` until live evidence proves an authorized change.
- Preserve every existing C1/C2/C3 working-tree change; no reset, restore, clean, overwrite or broad rewrite.
- Staging must remain empty. The user has not authorized stage or commit; this overrides generic plan-skill commit guidance.
- The current Codex Desktop controller remains owner until an explicit handoff is accepted.
- Worker completion is never controller acceptance. The controller inspects changed files and reruns relevant gates personally.
- No periodic monitor. A worker sends one completion/blocker signal; the controller reacts to that signal.
- Only one worker may write a given file. Every conditional repair receives a fresh Keeper boot, an exact file allowlist, exact tests and a completion contract.
- C3 scope remains one bounded UTF-8 create/modify proposal with explicit approve-once or reject. No terminal, Git, network tools, Obsidian, key management, extra runtime or allow-session permission.
- Do not reintroduce `taskkill.exe`, recursive PID-tree termination, numeric forced termination, name-wide cleanup or PID-only ownership.
- Do not start C4 until the C3 checkpoint is accepted by the controller and acknowledged by the user.

## Starting Evidence

- C1 checkpoint: `docs/verification/2026-07-31-wave-c1-checkpoint.md`.
- C2 checkpoint: `docs/verification/2026-08-01-wave-c2-checkpoint.md`.
- C3 design: `docs/superpowers/specs/2026-08-01-c3-governed-write-design.md`.
- C3 implementation status: Tasks 1-7 controller-accepted; checkpoint records
  written; C4 remains gated.
- Latest project-isolation repair: `docs/execution/2026-08-01-wave-c3f-project-isolation-fix-report.md`.
- Latest process-safety acceptance: `docs/execution/2026-08-01-desktop-process-cleanup-safety-fix-report.md`.
- Latest focused safety evidence: 29/29 synthetic tests passed.
- Final broad evidence: `pnpm check` passed with 21 Vitest files / 246 tests,
  Rust workspace/doctests and Clippy `-D warnings`; production UI build passed.
- Final C3 desktop state: focused C3 passed 1/1 with all eight scenarios and
  full serial desktop E2E passed 8/8. Process safety passed 41/41 and profile
  helper tests 4/4. Staging remains empty.

---

### Task 1: Controller Recovery Snapshot

**Owner:** Codex Desktop controller

**Files:**

- Read: `AGENTS.md`
- Read: `docs/handoff/2026-08-01-pillow-controller-handoff.md`
- Read: `docs/superpowers/specs/2026-08-01-c3-governed-write-design.md`
- Read: `docs/superpowers/plans/2026-08-01-c3-governed-write-implementation.md`
- Read: `docs/execution/2026-08-01-desktop-process-cleanup-safety-fix-report.md`
- Modify: none

**Interfaces:**

- Consumes: current Git state, Keeper controller session, Windows process baseline.
- Produces: a timestamped preflight record with repository root, branch, HEAD, staging count, dirty-entry count and relevant process list.

- [ ] **Step 1: Reconfirm Keeper ownership**

  Use the current controller session if it is healthy. If the task has restarted, POST a fresh controller boot for `project=pillow`; never reuse a stale worker lease.

- [ ] **Step 2: Capture Git truth**

  Run:

  ```powershell
  Set-Location this repository
  git rev-parse --show-toplevel
  git branch --show-current
  git rev-parse HEAD
  git status --short
  git diff --cached --name-only
  ```

  Expected: repository is `<repo>`, branch is `main`, staging output is empty, and the accumulated dirty tree is present. If HEAD or staging differs, reconcile the live history before continuing; do not force it back.

- [ ] **Step 3: Capture process truth**

  List Pillow, `fake_acp_agent`, Cargo and rustc processes with PID, path and parent evidence where available. Expected: no process owned by a previous Pillow acceptance remains. Unknown processes are reported, never killed.

- [ ] **Step 4: Record the gate decision**

  Continue only when repository/staging/process state is understood. A discrepancy is a controller blocker, not a worker repair task.

### Task 2: Process-Safety Requalification

**Owner:** Codex Desktop controller

**Files:**

- Inspect: `scripts/lib/desktop-process.mjs`
- Test: `scripts/lib/desktop-process-safety.test.mjs`
- Inspect: every C2/C3 orchestrator constructing `ProcessSupervisor`
- Modify: none on the green path

**Interfaces:**

- Consumes: clean Task 1 process baseline and the independently approved safety repair.
- Produces: permission to start one real desktop acceptance process.

- [ ] **Step 1: Run the focused synthetic safety suite**

  Run:

  ```powershell
  node --test scripts/lib/desktop-process-safety.test.mjs
  ```

  Expected: exit `0`, 29/29 tests passed.

- [ ] **Step 2: Recheck forbidden termination paths**

  Search Pillow scripts for `taskkill.exe`, recursive `/T` termination, numeric `SIGKILL`/`SIGTERM`, and name-wide process termination. Expected: no production cleanup path uses them.

- [ ] **Step 3: Validate construction contracts**

  Confirm every `ProcessSupervisor` root receives the launcher-owned `ChildProcess` handle and descendants remain observation-only identity records.

- [ ] **Step 4: Decide**

  A green result opens Task 3. Any failure stops real desktop execution and receives a dedicated safety brief; do not weaken fail-closed behavior to make the test pass.

### Task 3: Focused C3-F Real Desktop Acceptance

**Owner:** Codex Desktop controller

**Files:**

- Test: `apps/desktop/tests/e2e/c3-governed-write.spec.ts`
- Execute: `scripts/wave-c3-governed-write-acceptance.mjs`
- Execute: `scripts/lib/c3-governed-write-acceptance.mjs`
- Inspect: `apps/desktop/src/App.tsx`
- Inspect: `apps/desktop/src/features/workbench/Workbench.tsx`
- Modify: none on the green path

**Interfaces:**

- Consumes: Task 2 safety pass and the current project-isolation/harness corrections.
- Produces: one terminal C3-F record plus screenshots, scenario list, cleanup proof and failure diagnostics.

- [ ] **Step 1: Start one serial foreground run**

  Run:

  ```powershell
  node --test --test-concurrency=1 apps/desktop/tests/e2e/c3-governed-write.spec.ts
  ```

  Let the configured 30-minute test timeout govern the run. Do not add an outer short timeout and do not poll the command in the conversation.

- [ ] **Step 2: Validate the terminal record**

  Require exit `0` and a `KEEPER_WAVE_C3_GOVERNED_WRITE_ACCEPTANCE` record containing:

  ```text
  status = passed
  ipcBoundary = real-tauri-webview
  processCleanup = passed
  normalClose = passed
  forcedRestartCleanup = passed
  ```

- [ ] **Step 3: Validate all scenarios**

  Require all eight scenario names:

  ```text
  reject-create
  approve-create
  approve-modify
  base-drift
  cancellation
  conflict-replay
  two-projects
  forced-restart
  ```

- [ ] **Step 4: Inspect isolation semantics**

  Project B must never paint Project A's proposal, diff, terminal or resource evidence. When the user returns to Project A, Project A's own pending approval bar is expected and must not be misclassified as foreign evidence.

- [ ] **Step 5: Inspect privacy and authority**

  Confirm no approval grant, native project root, raw session/run/tool identifier, proposed content or provider-controlled raw output appears in persistence, logs, DOM, error text or screenshots.

- [ ] **Step 6: Recheck process baseline**

  Require normal application close and zero Pillow/`fake_acp_agent`/Cargo/rustc residual processes owned by the run. A cleanup ambiguity is a failure even when product assertions passed.

### Task 4: Conditional Failure Classification And Repair Dispatch

**Owner:** Codex Desktop controller for classification; one fresh worker for an authorized repair

**Files:**

- Create conditionally: `docs/execution/2026-08-01-wave-c3f-<failure-slug>-brief.md`
- Create conditionally: `docs/execution/2026-08-01-wave-c3f-<failure-slug>-report.md`
- Product/harness files: only the exact non-overlapping allowlist named after controller classification

**Interfaces:**

- Consumes: Task 3 terminal record, screenshots, logs, process identities and last-known-green scenario.
- Produces: either a no-code harness/evidence correction or one minimal TDD product repair ready for controller re-acceptance.

- [ ] **Step 1: Classify before dispatch**

  Assign exactly one primary class:

  ```text
  PRODUCT_RENDERER
  PRODUCT_NATIVE_RUNTIME_STORAGE
  HARNESS_ASSERTION_OR_DRIVER
  PROCESS_SAFETY
  HOST_OR_MACHINE
  ```

  A forced logout or missing terminal record alone does not prove a Pillow defect.

- [ ] **Step 2: Select one owner without overlapping writes**

  Routing preference:

  - renderer ownership, project isolation or approval UI: Grok product worker;
  - CDP driver, scenario assertion or artifact collection: Grok harness worker;
  - Rust/native/runtime/storage: one implementation worker selected after a Keeper health smoke;
  - process safety: dedicated safety worker plus a separate read-only reviewer;
  - host/machine issue: no product worker; controller gathers host evidence.

  Codex2 may own a bounded development task when explicitly assigned by the user. It never replaces the controller's final review or acceptance authority.

- [ ] **Step 3: Write a minimal repair brief**

  The brief must name the exact failing assertion, reproduction command, expected RED, exact writable files, forbidden files, input/output contract, focused tests, broad regression gate, privacy requirements and completion payload.

- [ ] **Step 4: Require TDD evidence**

  The worker must first reproduce the failure or add a focused failing regression, then make the minimum repair, then report changed files, RED/GREEN output, remaining risk, staging state and residual processes.

- [ ] **Step 5: Controller acceptance**

  Inspect every changed line, rerun focused tests, rerun Task 2 if process code changed, and rerun Task 3 from the beginning. Never accept a worker's completion message as the C3-F verdict.

### Task 5: Broad Controller Regression Gates

**Owner:** Codex Desktop controller

**Files:**

- Modify: none on the green path
- Evidence target: controller notes for the final C3 acceptance document

**Interfaces:**

- Consumes: a fully green Task 3 with no unresolved Important issue.
- Produces: broad workspace, build, formatting, desktop and process evidence for controller acceptance.

- [ ] **Step 1: Run the full repository check**

  ```powershell
  pnpm check
  ```

  Expected: workspace check, TypeScript, all Vitest suites, Rust workspace tests, doctests and Clippy `-D warnings` pass.

- [ ] **Step 2: Run the production UI build**

  ```powershell
  pnpm --filter @pillow/desktop build
  ```

  Expected: Vite production build exits `0`.

- [ ] **Step 3: Run all desktop E2E serially**

  ```powershell
  pnpm desktop:test:e2e
  ```

  Expected: conversation recovery, project launcher, C1 visual, C2 resource and C3 governed-write suites all pass with normal close and cleanup.

- [ ] **Step 4: Run repository hygiene gates**

  ```powershell
  cargo fmt --all --check
  git diff --check
  git diff --cached --name-only
  ```

  Expected: formatting/diff checks exit `0`; staging output is empty.

- [ ] **Step 5: Re-audit residual processes**

  Require no Pillow, `fake_acp_agent`, Cargo or rustc process owned by the gates. Preserve and report unknown processes rather than terminating them.

### Task 6: Grok4 And Codex2 Bounded Development

**Owner:** Grok4 and Codex2 as independent implementation workers; Codex Desktop as reviewer

**Files:**

- Grok4 allowlist: `apps/desktop/tests/write-approval-workbench.test.tsx` and its development report
- Codex2 allowlist: `scripts/lib/desktop-profile.mjs`, its focused test, the C3 orchestrator call site, and its development report

**Interfaces:**

- Consumes: two concrete reproduced harness/test weaknesses.
- Produces: two non-overlapping minimal repairs plus focused worker evidence for controller review.

- [ ] **Step 1: Dispatch both workers with explicit identities**

  Use `grok4` for expiry-fixture determinism and Keeper `codex2` bound to `gpt-5.6-terra`, high reasoning, `fastMode=false`, for the profile-snapshot helper API.

- [ ] **Step 2: Enforce non-overlapping write scope**

  Each worker may write only its brief's allowlist. Neither worker may stage, commit, run broad desktop acceptance, or edit checkpoint status.

- [ ] **Step 3: Require focused evidence and one completion signal**

  Each worker reports changed files, exact focused tests, residual risk, staging state and owned-process cleanup. No periodic monitor is created.

- [ ] **Step 4: Controller final review and acceptance reruns**

  The controller inspects every changed line, reruns both focused suites, reruns the real C3 desktop suite, and then personally executes Task 5 broad gates. Worker completion is not acceptance.

### Task 7: C3 Acceptance And Checkpoint Records

**Owner:** Codex Desktop controller

**Files:**

- Create: `docs/verification/2026-08-01-wave-c3-acceptance.md`
- Create: `docs/verification/2026-08-01-wave-c3-checkpoint.md`
- Modify: `README.md`
- Modify: `docs/roadmap/2026-07-31-stage1-overall-status.md`
- Modify: `docs/superpowers/plans/2026-08-01-c3-governed-write-implementation.md`
- Modify if needed: `docs/handoff/2026-08-01-pillow-controller-handoff.md`

**Interfaces:**

- Consumes: controller-reviewed Grok4/Codex2 changes and green controller evidence from all focused and broad gates.
- Produces: one internally consistent C3 checkpoint and an updated Stage 1 next-step boundary.

- [ ] **Step 1: Write the C3 acceptance record**

  Record exact commands/results, eight desktop scenarios, privacy/authority checks, process-safety evidence, changed-file scope, residual risks, HEAD and staging state. Do not summarize a failed or interrupted run as green.

- [ ] **Step 2: Write the checkpoint record**

  State what C3 makes product-available, what remains excluded, why restart/cancellation/project isolation are accepted, and that checkpoint documentation does not authorize a commit.

- [ ] **Step 3: Align status documents**

  Change Stage 1 status from `C3 implementation active` to `C3 accepted checkpoint` only if all prior gates passed. Set C4 to `next planning boundary`, not `implementation started`.

- [ ] **Step 4: Self-review documentation**

  Search the five affected documents for contradictory C3/C4 status, stale `C3-F active` language, placeholders, wrong test counts and any claim not backed by a terminal record.

- [ ] **Step 5: Re-run documentation hygiene**

  ```powershell
  git diff --check
  git diff --cached --name-only
  ```

  Expected: diff check passes and staging remains empty.

### Task 8: User Authorization Gate And C4 Preview

**Owner:** Codex Desktop controller

**Files:**

- Modify: none before user decision

**Interfaces:**

- Consumes: accepted C3 checkpoint documents and exact dirty-file manifest.
- Produces: a user decision on checkpoint staging/commit and, separately, permission to begin C4 planning.

- [ ] **Step 1: Report the checkpoint**

  Give the user the controller verdict, worker development summaries, exact tests, residual risks, HEAD, staging, residual processes and changed documentation paths.

- [ ] **Step 2: Request explicit checkpoint authority**

  Ask separately whether the user authorizes staging/committing the C1-C3 accumulated working tree. No answer means no stage/commit.

- [ ] **Step 3: Keep C4 gated**

  Even if checkpoint commit is authorized, C4 begins with a new research/design round for bounded terminal and Git authority. Do not infer C4 implementation permission from C3 acceptance.

## Completion Definition

This plan is complete only when:

- focused process safety is green;
- focused C3-F real desktop acceptance passes all eight scenarios;
- Grok4 and Codex2 bounded development is reviewed and independently reverified by the controller;
- full repository, production build, full desktop E2E, formatting and diff gates pass;
- residual process state is clean and ownership-safe;
- the Codex Desktop controller reports Critical 0 / Important 0 after source review;
- the controller records C3 acceptance and checkpoint without contradictions;
- staging remains empty unless the user explicitly authorizes otherwise;
- C4 has not started accidentally.

## Planned Round Brief

| Order | Task | Responsible party | Dispatch state |
| --- | --- | --- | --- |
| 1 | Recovery snapshot and process-safety requalification | Codex Desktop controller | Completed; 41/41 final safety suite |
| 2 | Focused C3-F real desktop acceptance | Codex Desktop controller | Completed; 1/1 and all 8 scenarios |
| 3 | Expiry-fixture determinism development | Grok4 CLI | Completed and controller-accepted |
| 4 | Harness fixture development | Account-2 Codex CLI (`gpt-5.6-terra`, high) | Completed; controller accepted run-cargo and desktop-smoke fixes |
| 5 | Source review, focused reruns, broad repository and desktop gates | Codex Desktop controller | Completed; Critical 0 / Important 0; desktop 8/8 |
| 6 | Acceptance/checkpoint documentation | Codex Desktop controller | Completed in 2026-08-02 records |
| 7 | Stage/commit and C4 decision | User + controller | Still requires explicit authorization |
