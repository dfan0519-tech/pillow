# C4 Next-Round Controller Orchestration Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move C4 from its completed D1 protocol/policy/parser/UI foundations through the governed process and read-only Git product loops, then reach a controller-owned C4.0 acceptance checkpoint.

**Architecture:** Keep `process.run@1` and Git read as separate authorities. A shared stable-handle Windows process runner is consumed by a metadata-only governed process gateway and by fixed-template Git reads; Tauri/TypeScript bridges project sanitized DTOs into the existing conversation-centred workbench. Each dependency edge is accepted by the controller before the next writer receives its lease.

**Tech Stack:** Rust 2024, Tauri 2, rusqlite 0.32, windows-sys 0.59, React 19, TypeScript 5.8, Vitest 3, Node desktop E2E.

## Global Constraints

- Work only in `<workspace>\pillow`; `<workspace>\VaultSystem\keeper` and reference clones remain read-only.
- Preserve HEAD `7b7f70f` and every accumulated accepted C1–C3 working-tree change.
- Do not stage, commit, reset, restore, clean, package or publish.
- C4.0 permits one approve-once structured process and project-scoped read-only Git. It excludes PTY, shell strings, arbitrary executables, Git mutation, credentials, network, runtime expansion, Obsidian and Stage 3 work.
- No numeric-PID, process-name-wide or best-effort termination. Windows ownership and cleanup must use stable child/Job Object handles and fail closed when proof is incomplete.
- Real external workers are used: Grok CLI slots `grok1`–`grok4` and account-2 Codex CLI. Local Codex subagents are not substitutes for those slots.
- Every worker boots through Keeper, receives one exclusive write allowlist, runs RED -> GREEN focused tests, writes one completion report, sends one completion signal, then releases its lease. Do not create a periodic conversation monitor.
- The controller writes no mainline feature implementation. The controller owns briefs, dependency gates, source review, integration arbitration, full test gates and the only acceptance verdict.

## Current Entry State

- D1 protocol, policy, Git semantic parser and isolated renderer primitives are implemented.
- D1R corrected public exports, argv schema/display, read-executor denial, repository identity validation and workspace lockfile integration.
- D1R2 corrected the two `clippy::byte-char-slices` findings; package tests and package Clippy are green.
- HEAD remains `7b7f70f`; cached diff remains empty.
- D1 is not accepted until the controller reruns the exact aggregate gates below.

---

### Task 0: Controller D1 acceptance gate

**Files:**
- Review: `Cargo.toml`
- Review: `Cargo.lock`
- Review: `crates/agent-protocol/`
- Review: `crates/kernel-policy/`
- Review: `crates/git-executor/`
- Review: `crates/tool-executor/`
- Review: `apps/desktop/src/features/execution/`
- Review: `apps/desktop/tests/c4-execution-primitives.test.tsx`
- Update only after all gates pass: `.superpowers/sdd/progress.md`

**Produces:** a D1 controller verdict. No worker is dispatched from this task.

- [ ] Review every D1/D1R/D1R2 report against its actual allowlist and inspect the complete new Git crate with an untracked-file-aware file listing.
- [ ] Run `corepack pnpm check`; require all renderer tests, Rust workspace tests/doctests and workspace Clippy `-D warnings` to exit 0.
- [ ] Run `corepack pnpm --filter @pillow/desktop build`, `cargo fmt --all --check`, `git diff --check`, and verify `git diff --cached --name-only` is empty.
- [ ] Verify no Pillow-owned test/worker descendants or listeners remain. Observation-only PIDs must not be killed; ownership uncertainty is a rejection.
- [ ] Record `ACCEPT` only with Critical 0 / Important 0. Otherwise issue a minimal repair brief to the owning external worker.

### Task 1: D2A shared supervised process executor

**Owner:** actual account-2 Codex CLI, `gpt-5.6-terra`, high reasoning, `fastMode=false`.

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/process-executor/Cargo.toml`
- Create: `crates/process-executor/src/lib.rs`
- Create: `crates/process-executor/src/env.rs`
- Create: `crates/process-executor/src/output.rs`
- Create: `crates/process-executor/src/job_windows.rs`
- Create: `crates/process-executor/tests/supervised_process.rs`
- Modify: `crates/adapter-grok-acp/Cargo.toml`
- Modify: `crates/adapter-grok-acp/src/process/child.rs`
- Modify: `crates/adapter-grok-acp/src/process/job_windows.rs`
- Create: `docs/execution/2026-08-02-wave-c4-d2a-codex2-process-executor-report.md`

**Produces:** `ProcessSpec`, `ProcessReceipt`, `CancellationSignal`, `SupervisedProcessRunner`.

- [ ] Add failing lifecycle tests for stdout/stderr, lossy UTF-8, truncation, timeout, cancellation, grandchild cleanup, pre-resume spawn failure and secret-environment exclusion.
- [ ] Extract the accepted Windows Job Object implementation into the new crate without weakening the Grok ACP adapter behavior.
- [ ] Require clean-environment construction and owned-handle cleanup proof; forbid `taskkill` and numeric-PID termination symbols.
- [ ] Run process-executor tests, ACP adapter tests, workspace check/Clippy, fmt and scoped diff-check.
- [ ] Controller reviews stable-handle ownership and reruns focused tests before Task 3 or Task 4 starts.

### Task 2: D2A metadata-only execution ledger

**Owner:** real Grok CLI slot `grok1`, parallel with Task 1.

**Files:**
- Create: `crates/storage-sqlite/migrations/0007_exec_proposals.sql`
- Create: `crates/storage-sqlite/src/exec_proposal.rs`
- Modify: `crates/storage-sqlite/src/lib.rs`
- Modify: `crates/storage-sqlite/src/migrations.rs`
- Create: `crates/storage-sqlite/tests/exec_proposal_repository.rs`
- Create: `docs/execution/2026-08-02-wave-c4-d2a-grok1-exec-ledger-report.md`

**Produces:** reservation, decision, run-claim, terminal receipt and restart-recovery repository APIs.

- [ ] Add RED migration/repository tests for replay/conflict, one non-terminal per run, single-use decision, expiry, restart interruption and conversation-scoped listing.
- [ ] Add byte scans proving SQLite does not retain argv, native paths, raw grants, environment or full transcript.
- [ ] Implement the metadata/digest-only schema with a partial unique index for non-terminal proposals.
- [ ] Run the full storage suite, workspace check, fmt and scoped diff-check.
- [ ] Controller reviews schema privacy and migration ordering before Task 3 starts.

### Task 3: D2B governed process gateway

**Owner:** real Grok CLI slot `grok2`; starts only after Tasks 1 and 2 are controller-accepted.

**Files:**
- Modify: `crates/tool-executor/Cargo.toml`
- Create: `crates/tool-executor/src/process.rs`
- Modify: `crates/tool-executor/src/lib.rs`
- Create: `crates/tool-executor/tests/governed_process.rs`
- Create: `docs/execution/2026-08-02-wave-c4-d2b-grok2-process-gateway-report.md`

**Produces:** `SealedProcessSpec`, `PreparedExecProposal`, `GovernedProcessOutcome`, `GovernedProcessError`, `GovernedProcessToolGateway`.

- [ ] Add RED cases for allowlisted labels, shell/Git/script denial, fingerprint drift, approve/reject/expiry, clean environment, timeout/cancel, replay and cleanup uncertainty.
- [ ] Implement seal -> decide -> claim -> execute -> terminal receipt using only `SupervisedProcessRunner` for spawning.
- [ ] Keep resolved executable, argv and native cwd memory-only; persist only their approved digests and portable metadata.
- [ ] Run executor tests, workspace check/Clippy, fmt and scoped diff-check.
- [ ] Controller verifies that Git cannot enter through `process.run@1`.

### Task 4: D2B hardened Git read runner

**Owner:** real Grok CLI slot `grok4`, parallel with Task 3; starts only after Task 1 and D1 Git parser acceptance.

**Files:**
- Modify: `crates/git-executor/Cargo.toml`
- Create: `crates/git-executor/src/runner.rs`
- Create: `crates/git-executor/src/discovery.rs`
- Modify: `crates/git-executor/src/lib.rs`
- Create: `crates/git-executor/tests/git_read_integration.rs`
- Create: `docs/execution/2026-08-02-wave-c4-d2b-grok4-git-runner-report.md`

**Produces:** `GitReadExecutor::discover`, `GitReadExecutor::status`, `GitReadExecutor::diff` with fixed invocation templates.

- [ ] Add RED real-repository fixtures for rename, binary, conflict, detached HEAD, nested repository, outside-root linked worktree, unsafe hooks/filters/textconv, large diff and generation drift.
- [ ] Resolve one trusted `git.exe`; use clean environment, disabled prompt/pager/external diff/textconv and machine-readable porcelain v2 `-z` templates.
- [ ] Expose semantic operations only; never accept or return arbitrary Git argv or native roots.
- [ ] Run the full Git suite, workspace check/Clippy, fmt and scoped diff-check.
- [ ] Controller reviews every fixed Git template and unsafe-configuration denial before integration.

### Task 5: D3A runtime-host process coordination

**Owner:** real Grok CLI slot `grok1`; starts after Task 3 acceptance.

**Files:**
- Modify: `crates/runtime-host/Cargo.toml`
- Create: `crates/runtime-host/src/host/process.rs`
- Modify: `crates/runtime-host/src/host/mod.rs`
- Modify: `crates/runtime-host/src/lib.rs`
- Create: `crates/runtime-host/tests/governed_process_bridge.rs`
- Create: `docs/execution/2026-08-02-wave-c4-d3a-grok1-runtime-process-report.md`

**Produces:** provider proposal registration/release, decision, output-open and terminal event coordination.

- [ ] Add RED tests for provider ownership injection, one pending action, cross-conversation denial, cancel ordering, replay, restart interruption and zero residual host state.
- [ ] Keep process coordination types separate from governed read/write maps.
- [ ] Preserve the accepted C3 cancellation law: at most one validated terminal process receipt before a host-forced run terminal event.
- [ ] Run runtime-host tests, workspace check/Clippy, fmt and scoped diff-check.
- [ ] Controller reviews event ordering before native gateways start.

### Task 6: D3B typed native and TypeScript gateways

**Owner:** real Grok CLI slot `grok2`; starts after Tasks 4 and 5 acceptance.

**Files:**
- Create: `apps/desktop/src-tauri/src/exec_commands.rs`
- Create: `apps/desktop/src-tauri/src/git_commands.rs`
- Create: `apps/desktop/src-tauri/src/conversation_service/exec_service.rs`
- Create: `apps/desktop/src-tauri/src/conversation_service/git_service.rs`
- Modify: `apps/desktop/src-tauri/src/conversation_service/mod.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Create: `apps/desktop/src/lib/execGateway.ts`
- Create: `apps/desktop/src/lib/gitGateway.ts`
- Create: `apps/desktop/src/lib/tauriExecGateway.ts`
- Create: `apps/desktop/src/lib/tauriGitGateway.ts`
- Create: `apps/desktop/tests/c4-gateway-contract.test.ts`
- Create: `docs/execution/2026-08-02-wave-c4-d3b-grok2-gateway-report.md`

**Produces:** strict process list/decide/output and Git discover/status/diff DTO parity.

- [ ] Add RED Rust/TS parity tests rejecting invalid enums, unknown fields, native paths, raw grants/IDs, oversized output and stale generations.
- [ ] Require conversation capability for process APIs and project capability for Git APIs.
- [ ] Expose only portable paths, fixed enums and opaque public errors.
- [ ] Run desktop native tests, focused renderer contract tests, typecheck, production build, workspace check and scoped diff-check.
- [ ] Controller reviews every native-to-TS field before product mounting.

### Task 7: D3C product mounting and adversarial harness

**Owner A:** real Grok CLI slot `grok3` for workbench mounting.

**Owner A files:**
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/features/workbench/Workbench.tsx`
- Modify: `apps/desktop/src/features/workbench/ResourcePane.tsx`
- Modify: `apps/desktop/src/features/conversations/RunActivity.tsx`
- Modify: `apps/desktop/src/styles/workbench-conversation.css`
- Modify: `apps/desktop/src/styles/workbench-shell.css`
- Modify: `apps/desktop/src/styles/execution.css`
- Create: `apps/desktop/tests/c4-workbench-integration.test.tsx`
- Create: `docs/execution/2026-08-02-wave-c4-d3c-grok3-workbench-report.md`

**Owner B:** real Grok CLI slot `grok4` for the RED desktop harness, parallel only after Task 6 contracts are fixed.

**Owner B files:**
- Create: `apps/desktop/tests/e2e/c4-governed-process-git.spec.ts`
- Create: `scripts/wave-c4-governed-process-git-acceptance.mjs`
- Modify: `apps/desktop/package.json`
- Create: `docs/execution/2026-08-02-wave-c4-d3c-grok4-e2e-harness-report.md`

- [ ] Grok3 adds RED ownership/race tests, then mounts one command approval bar plus contextual Output/Changes modes in the existing workbench.
- [ ] The UI selects the oldest pending write/process action deterministically and never renders two approval bars or a second dashboard.
- [ ] Grok4 adds RED real desktop scenarios for reject, success, nonzero exit, expiry, cancel, timeout, bounded/lossy output, grandchild cleanup, isolation/restart, Git rename/binary/nested repository, unsafe config and generation drift.
- [ ] Harness-only corrections remain in Owner B's allowlist; product failures return to the owning task through a new repair brief.
- [ ] Controller reviews both slices, runs focused integration/E2E, and inspects deterministic 1024x720 and 1440x960 screenshots.

### Task 8: Controller repair arbitration and C4.0 final acceptance

**Owner:** controller. Account-2 Codex CLI may receive one bounded high-risk development repair; it is not the final reviewer.

**Files:**
- Create only after all gates pass: `docs/verification/2026-08-02-wave-c4-acceptance.md`
- Create only after acceptance: `docs/verification/2026-08-02-wave-c4-checkpoint.md`
- Update only after acceptance: `docs/roadmap/2026-07-31-stage1-overall-status.md`
- Update only after acceptance: `.superpowers/sdd/progress.md`

- [ ] Review all changed and untracked C4 files, reports and privacy/termination invariants; require Critical 0 / Important 0.
- [ ] Send every defect to exactly one owning worker with a non-overlapping repair allowlist and focused RED/GREEN contract.
- [ ] Run `corepack pnpm check`, production desktop build, focused C4 desktop acceptance, full serial desktop E2E, `cargo fmt --all --check`, `git diff --check`, cached-diff check and residual-process/listener proof.
- [ ] Inspect both target-resolution screenshots for conversation dominance, one approval bar, progressive disclosure and absence of raw/native metadata.
- [ ] Write acceptance/checkpoint only after every controller-run command is green. Do not stage or commit without separate user authorization.

## Dependency and Dispatch Order

1. Controller closes Task 0.
2. Dispatch Task 1 to Codex2 and Task 2 to Grok1 in parallel.
3. After both controller gates, dispatch Task 3 to Grok2 and Task 4 to Grok4 in parallel.
4. Dispatch Task 5 to Grok1, then Task 6 to Grok2 after its input contracts are accepted.
5. Dispatch Task 7 to Grok3 and Grok4 with disjoint allowlists.
6. Controller arbitrates repairs and runs Task 8.

No later round starts merely because files exist. Each dependency requires a completion report, actual diff review and fresh focused controller evidence.
