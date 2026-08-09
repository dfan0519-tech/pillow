# C4 Governed Process and Git Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the accepted C4.0 checkpoint: one approve-once structured process run plus project-scoped read-only Git discovery, status and bounded diff, integrated into Pillow's conversation-centred desktop workbench.

**Architecture:** `agent-protocol` and `kernel-policy` define authority; a new `process-executor` owns stable Windows child/Job Object execution; `tool-executor` seals and applies process proposals through a metadata-only SQLite ledger; a separate `git-executor` uses fixed Git templates over the same low-level process runner. Tauri exposes typed conversation/project DTOs, while React renders one approval bar and contextual output/Changes modes.

**Tech Stack:** Rust 2024, Tauri 2, rusqlite 0.32, windows-sys 0.59, React 19, TypeScript 5.8, Vitest 3, Node desktop E2E.

## Global Constraints

- Work only in `this repository`; `legacy Keeper (out of tree)` and local reference clones are read-only.
- Preserve HEAD `7b7f70f` and the accumulated dirty C1-C3 working tree.
- Do not stage, commit, reset, restore, clean, package or publish.
- C4.0 contains `process.run@1` and read-only Git only. It contains no PTY, shell string, Git mutation, network, credentials, Obsidian or runtime expansion.
- Provider input contains no native root, executable path, environment, approval grant or Git argv.
- Process execution is structured argv only; `git`, shells and script shims are denied by the public process catalog.
- Windows cleanup uses stable handles and an owned Job Object. No `taskkill`, numeric-PID termination or name-wide cleanup.
- SQLite stores metadata, digests and terminal receipts, never full transcript, native root/executable, raw grant or arbitrary environment.
- UI stays conversation-centred; Git and process output are modes of the existing collapsible context pane.
- Every task follows RED -> GREEN -> focused regression and reports changed files, tests, residual risks, staging state and process residuals.
- Parallel tasks have non-overlapping write allowlists. Common entry-point edits are reserved for their named integration task.

## File and ownership map

| Unit | Responsibility | Owner task |
| --- | --- | --- |
| `crates/agent-protocol/src/tool/*` | fixed `process.run@1` wire/result validation | Task 1 |
| `crates/kernel-policy/src/process.rs` | process proposal state machine | Task 2 |
| `crates/git-executor/` | Git semantic DTO/parser and later runner | Tasks 3, 9 |
| `apps/desktop/src/features/execution/` | isolated C4 UI primitives | Task 4 |
| `crates/process-executor/` | supervised child and Job Object | Task 5 |
| `crates/storage-sqlite/src/exec_proposal.rs` | metadata-only process ledger | Task 6 |
| `crates/tool-executor/src/process.rs` | sealed proposal and apply gateway | Task 7 |
| `crates/runtime-host/src/host/process.rs` | runtime proposal coordination | Task 8 |
| native/TS C4 gateways | typed renderer bridge | Task 10 |
| `Workbench` / contextual pane integration | mounted product experience | Task 11 |
| desktop E2E and acceptance scripts | real adversarial proof | Task 12 |

---

### Task 1: `process.run@1` protocol contract

**Files:**
- Modify: `crates/agent-protocol/src/tool/types.rs`
- Modify: `crates/agent-protocol/src/tool/manifest.rs`
- Modify: `crates/agent-protocol/src/tool/wire.rs`
- Modify: `crates/agent-protocol/src/tool/validate.rs`
- Test: `crates/agent-protocol/tests/tool_contract.rs`
- Test: `crates/agent-protocol/tests/tool_contract_security.rs`

**Interfaces:**
- Produces: `ToolId::ProcessRun`, `ProcessRunArguments`, `ProcessRunOutput`, `ToolArguments::ProcessRun`, `ToolResult::ProcessRun`.
- Constants: `PROCESS_ARGV_COUNT_CAP = 64`, `PROCESS_ARGV_BYTE_CAP = 32_768`.

- [ ] **Step 1: Add RED contract and security cases**

Add fixtures asserting exact `{program, argv, cwd}` acceptance, unknown-field denial, program-label validation, NUL/control rejection, 64/65 argv boundary, 32 KiB boundary, portable cwd validation and exact result fields. The core type shape must be:

```rust
pub const PROCESS_ARGV_COUNT_CAP: usize = 64;
pub const PROCESS_ARGV_BYTE_CAP: usize = 32_768;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessRunArguments {
    pub program: String,
    pub argv: Vec<String>,
    pub cwd: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessRunOutput {
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub captured_bytes: u64,
    pub truncated: bool,
    pub encoding_lossy: bool,
    pub cancelled: bool,
    pub timed_out: bool,
}
```

- [ ] **Step 2: Run RED tests**

Run: `cargo test -p keeper-agent-protocol --tests process_run -- --nocapture`

Expected: FAIL because `ToolId::ProcessRun` and process argument/result variants do not exist.

- [ ] **Step 3: Implement the fixed manifest and validator**

Add `ToolId::ProcessRun` with wire name `process.run@1`. The manifest requires approval, has fixed 60,000 ms timeout and returns metadata-only output. Reuse the existing portable path validator for `cwd`; validate `program` with `^[a-z][a-z0-9_-]{0,31}$`; reject unknown keys and noncanonical result JSON.

- [ ] **Step 4: Run GREEN and protocol regressions**

Run:

```text
cargo test -p keeper-agent-protocol --tests
cargo fmt --all --check
git diff --check -- crates/agent-protocol
```

Expected: all agent-protocol tests pass; formatting and diff checks exit 0.

- [ ] **Step 5: Report without staging or commit**

Return the six changed files, RED/GREEN commands, exact test counts and any validator ambiguity. Confirm cached diff is empty.

### Task 2: Process approval policy

**Files:**
- Create: `crates/kernel-policy/src/process.rs`
- Modify: `crates/kernel-policy/src/lib.rs`
- Create: `crates/kernel-policy/tests/process_policy.rs`

**Interfaces:**
- Produces: `ExecProposalState`, `ExecDecision`, `ExecPolicyError`, `validate_exec_transition`, `apply_exec_decision`, `EXEC_PROPOSAL_TTL_SECS`.

- [ ] **Step 1: Write the RED state-matrix test**

The exact state model is:

```rust
pub const EXEC_PROPOSAL_TTL_SECS: i64 = 600;

pub enum ExecProposalState {
    PendingApproval,
    Approved,
    Running,
    Succeeded,
    Failed,
    Rejected,
    Expired,
    Cancelled,
    Interrupted,
}

pub enum ExecDecision { ApproveOnce, Reject }
```

Tests must enumerate every allowed and denied edge, expiry-before-decision,
single-use decision, restart interruption and terminal immutability.

- [ ] **Step 2: Run RED**

Run: `cargo test -p keeper-policy --test process_policy -- --nocapture`

Expected: compile failure because the process policy module is absent.

- [ ] **Step 3: Implement pure policy functions**

Mirror the accepted write-policy style but do not share write enums. `Approved -> Running` is the only execution claim; `Running -> Succeeded|Failed|Cancelled|Interrupted`; pending/approved may expire/cancel/interrupt; all terminal states reject further transitions.

- [ ] **Step 4: Run GREEN and full policy regression**

Run:

```text
cargo test -p keeper-policy --tests
cargo fmt --all --check
git diff --check -- crates/kernel-policy
```

Expected: process matrix and existing write policy pass.

- [ ] **Step 5: Report without staging or commit**

Return changed files and state-matrix counts; confirm no write-policy semantics changed.

### Task 3: Read-only Git semantic parser foundation

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/git-executor/Cargo.toml`
- Create: `crates/git-executor/src/lib.rs`
- Create: `crates/git-executor/src/status.rs`
- Create: `crates/git-executor/src/diff.rs`
- Create: `crates/git-executor/tests/status_porcelain.rs`
- Create: `crates/git-executor/tests/bounded_diff.rs`

**Interfaces:**
- Produces: `GitRepositoryId`, `GitHead`, `GitStatusKind`, `GitStatusEntry`, `GitStatusSnapshot`, `GitDiffView`, `parse_porcelain_v2_z`, `bound_git_diff`.

- [ ] **Step 1: Write RED byte-fixture parser tests**

Use inline NUL-delimited byte fixtures for ordinary, untracked, rename,
conflict, submodule and detached-HEAD records. Lock this DTO:

```rust
pub struct GitStatusSnapshot {
    pub repository: GitRepositoryId,
    pub head: GitHead,
    pub entries: Vec<GitStatusEntry>,
    pub truncated: bool,
    pub generation: u64,
}
```

All paths are portable project-relative strings. A rename is one entry with
`old_path: Option<String>` and `path: String`.

- [ ] **Step 2: Run RED**

Run: `cargo test -p keeper-git-executor --tests -- --nocapture`

Expected: package/module compile failure.

- [ ] **Step 3: Implement pure parsing and bounds only**

Add the workspace member and a crate with no process launch. Parse porcelain
v2 `-z` without lossy splitting, reject malformed/trailing partial records,
cap status at 5,000 entries/2 MiB, and cap one text diff at 256 KiB. Binary and
oversized diff return typed summaries, never partial success.

- [ ] **Step 4: Run GREEN**

Run:

```text
cargo test -p keeper-git-executor --tests
cargo check --workspace --all-targets
cargo fmt --all --check
git diff --check -- Cargo.toml crates/git-executor
```

Expected: parser tests and workspace check pass; no new external crate is added.

- [ ] **Step 5: Report without staging or commit**

Return parser coverage, malformed inputs and any Git version assumption.

### Task 4: Isolated renderer primitives

**Files:**
- Create: `apps/desktop/src/features/execution/executionTypes.ts`
- Create: `apps/desktop/src/features/execution/CommandActivityRow.tsx`
- Create: `apps/desktop/src/features/execution/CommandApprovalBar.tsx`
- Create: `apps/desktop/src/features/execution/TerminalOutputView.tsx`
- Create: `apps/desktop/src/features/execution/GitChangesView.tsx`
- Create: `apps/desktop/src/styles/execution.css`
- Create: `apps/desktop/tests/c4-execution-primitives.test.tsx`

**Interfaces:**
- Produces: `CommandProposalView`, `CommandActivityView`, `GitStatusView`, `GitDiffView` and four controlled components. No Tauri imports.

- [ ] **Step 1: Write RED interaction/privacy tests**

Tests require: one compact activity row; literal argument display; approve-once/reject/review callbacks; no grant/native path/raw IDs in DOM; output cap/truncation/lossy labels; Git rename/binary/stale states; keyboard focus and Escape rejection only while the bar owns focus.

- [ ] **Step 2: Run RED**

Run: `corepack pnpm --filter @pillow/desktop test -- c4-execution-primitives.test.tsx`

Expected: FAIL because the execution feature does not exist.

- [ ] **Step 3: Implement controlled components**

Use these discriminants:

```ts
export type CommandOrigin = "agent" | "user";
export type CommandState =
  | "pending_approval" | "approved" | "running" | "succeeded"
  | "failed" | "rejected" | "expired" | "cancelled" | "interrupted";
```

Components receive already-sanitized DTOs and callbacks. They do not fetch,
persist, infer terminal truth or own project/conversation selection.

- [ ] **Step 4: Run GREEN and renderer regression**

Run:

```text
corepack pnpm --filter @pillow/desktop test -- c4-execution-primitives.test.tsx
corepack pnpm --filter @pillow/desktop typecheck
corepack pnpm --filter @pillow/desktop build
git diff --check -- apps/desktop/src/features/execution apps/desktop/src/styles/execution.css apps/desktop/tests/c4-execution-primitives.test.tsx
```

Expected: focused tests, typecheck and build pass.

- [ ] **Step 5: Report without staging or commit**

Return screenshots only if produced through a deterministic test harness; confirm no product mount or global CSS edit.

### Task 5: Shared supervised process executor

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

**Interfaces:**
- Consumes: no C4 protocol or storage types.
- Produces: `ProcessSpec`, `ProcessReceipt`, `CancellationSignal`, `SupervisedProcessRunner`.

- [ ] **Step 1: Write RED lifecycle tests**

Lock this boundary:

```rust
pub struct ProcessSpec {
    pub executable: PathBuf,
    pub argv: Vec<OsString>,
    pub cwd: PathBuf,
    pub env: BTreeMap<OsString, OsString>,
    pub timeout: Duration,
    pub output_byte_cap: usize,
}

pub struct ProcessReceipt {
    pub exit_code: Option<i32>,
    pub duration: Duration,
    pub stdout: BoundedOutput,
    pub stderr: BoundedOutput,
    pub cancelled: bool,
    pub timed_out: bool,
    pub cleanup_proven: bool,
}
```

Tests cover stdout/stderr, lossy UTF-8, truncation, timeout, cancellation,
grandchild cleanup, spawn failure before resume and secret-env absence.

- [ ] **Step 2: Run RED**

Run: `cargo test -p keeper-process-executor --tests -- --nocapture`

Expected: package absent.

- [ ] **Step 3: Extract the accepted Windows Job Object implementation**

Create suspended, assign to kill-on-close Job Object, start capture, resume,
wait and prove cleanup. Root termination uses the owned child/Job handles only.
Move adapter-Grok to the shared implementation without changing its public
behavior or weakening existing tests.

- [ ] **Step 4: Run GREEN and ACP regressions**

Run:

```text
cargo test -p keeper-process-executor --tests
cargo test -p keeper-adapter-grok-acp --tests
cargo check --workspace --all-targets
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
git diff --check -- Cargo.toml crates/process-executor crates/adapter-grok-acp
```

Expected: all pass; no forbidden termination symbol appears.

- [ ] **Step 5: Report without staging or commit**

Report stable-handle ownership, residual proof and any Windows-only gap.

### Task 6: Metadata-only execution ledger

**Files:**
- Create: `crates/storage-sqlite/migrations/0007_exec_proposals.sql`
- Create: `crates/storage-sqlite/src/exec_proposal.rs`
- Modify: `crates/storage-sqlite/src/lib.rs`
- Modify: `crates/storage-sqlite/src/migrations.rs`
- Create: `crates/storage-sqlite/tests/exec_proposal_repository.rs`

**Interfaces:**
- Consumes: Task 1 protocol identity and Task 2 policy state.
- Produces: `StoredExecProposal`, `ExecProposalReserveInput`, `ExecReservationOutcome`, `ExecDecisionOutcome`, `ExecRunClaimOutcome`, `ExecTerminalReceipt`, `SqliteExecProposalRepository`.

- [ ] **Step 1: Write RED migration/repository tests**

Tests cover reservation/replay/conflict, one non-terminal per run, decision
single-use, claim-running lease, terminal receipt, expiry, restart recovery,
conversation listing and database scans proving no argv, native path, grant,
environment or transcript bytes are present.

- [ ] **Step 2: Run RED**

Run: `cargo test -p keeper-storage-sqlite --test exec_proposal_repository -- --nocapture`

Expected: migration/repository absent.

- [ ] **Step 3: Implement migration and repository**

Store program label, argv digest, portable cwd, fingerprint, lifecycle,
timestamps, grant digest, exit metadata, byte counts, stream digests and bounded
result JSON. Use a partial unique index for one non-terminal proposal per run.

- [ ] **Step 4: Run GREEN and storage regression**

Run:

```text
cargo test -p keeper-storage-sqlite --tests
cargo check --workspace --all-targets
cargo fmt --all --check
git diff --check -- crates/storage-sqlite
```

- [ ] **Step 5: Report without staging or commit**

Include schema columns, uniqueness proof and privacy scan evidence.

### Task 7: Governed process gateway

**Files:**
- Modify: `crates/tool-executor/Cargo.toml`
- Create: `crates/tool-executor/src/process.rs`
- Modify: `crates/tool-executor/src/lib.rs`
- Create: `crates/tool-executor/tests/governed_process.rs`

**Interfaces:**
- Consumes: Tasks 1, 2, 5 and 6.
- Produces: `SealedProcessSpec`, `PreparedExecProposal`, `GovernedProcessOutcome`, `GovernedProcessError`, `GovernedProcessToolGateway`.

- [ ] **Step 1: Write RED end-to-end gateway tests**

Cover tool-label allowlist, git/shell/script denial, argv/cwd fingerprint,
approve/reject/expiry, program-resolution drift, secret env exclusion,
timeout/cancel, receipt replay and cleanup uncertainty.

- [ ] **Step 2: Run RED**

Run: `cargo test -p keeper-tool-executor --test governed_process -- --nocapture`

- [ ] **Step 3: Implement seal -> decide -> claim -> execute -> receipt**

The sealed executable/argv/cwd remains memory-only. The store gets only
digests/metadata. Re-resolve cwd and executable at execution; fail closed on
drift. Use `SupervisedProcessRunner` and never spawn directly in this crate.

- [ ] **Step 4: Run GREEN and executor regression**

Run:

```text
cargo test -p keeper-tool-executor --tests
cargo check --workspace --all-targets
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
git diff --check -- crates/tool-executor
```

Expected: all commands exit 0.

- [ ] **Step 5: Report without staging or commit**

Report denial matrix and demonstrate that Git cannot enter through process.run.

### Task 8: Runtime-host process coordination

**Files:**
- Modify: `crates/runtime-host/Cargo.toml`
- Create: `crates/runtime-host/src/host/process.rs`
- Modify: `crates/runtime-host/src/host/mod.rs`
- Modify: `crates/runtime-host/src/lib.rs`
- Create: `crates/runtime-host/tests/governed_process_bridge.rs`

**Interfaces:**
- Consumes: `GovernedProcessToolGateway`.
- Produces: register/release process run, list/decide/open-output methods and validated runtime events.

- [ ] **Step 1: Write RED bridge tests**

Prove provider proposal ownership injection, one pending action, cancellation
ordering, terminal replay, cross-conversation denial, restart interruption and
zero residual worker/slot state.

- [ ] **Step 2: Run RED**

Run: `cargo test -p keeper-runtime-host --test governed_process_bridge -- --nocapture`

- [ ] **Step 3: Implement coordination**

Add process maps beside governed read/write maps but keep types separate. A
cancelled turn may deliver at most one validated terminal process receipt
before the host-forced run terminal event, following the accepted C3 rule.

- [ ] **Step 4: Run GREEN and runtime regression**

Run:

```text
cargo test -p keeper-runtime-host --tests
cargo check --workspace --all-targets
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
git diff --check -- crates/runtime-host
```

Expected: all commands exit 0.

- [ ] **Step 5: Report without staging or commit**

Report event ordering and all residual counts.

### Task 9: Hardened Git command runner

**Files:**
- Modify: `crates/git-executor/Cargo.toml`
- Create: `crates/git-executor/src/runner.rs`
- Create: `crates/git-executor/src/discovery.rs`
- Modify: `crates/git-executor/src/lib.rs`
- Create: `crates/git-executor/tests/git_read_integration.rs`

**Interfaces:**
- Consumes: `SupervisedProcessRunner` from Task 5 and parsers from Task 3.
- Produces: `GitReadExecutor::discover`, `status`, `diff` with fixed templates.

- [ ] **Step 1: Write RED real-repository fixtures**

Use temporary repositories for nested repo, rename, binary, conflict,
detached-HEAD, linked worktree outside root, unsafe hook/filter/textconv config,
large diff and generation drift.

- [ ] **Step 2: Run RED**

Run: `cargo test -p keeper-git-executor --test git_read_integration -- --nocapture`

- [ ] **Step 3: Implement fixed Git templates**

Resolve one trusted `git.exe`; use clean env, no prompt/pager, porcelain v2
`-z`, `--no-ext-diff`, `--no-textconv` and bounded output. Do not expose an
argv API. Reject unsafe local executable config and outside-root worktrees.

- [ ] **Step 4: Run GREEN and full Git regression**

Run:

```text
cargo test -p keeper-git-executor --tests
cargo check --workspace --all-targets
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
git diff --check -- crates/git-executor
```

Expected: all commands exit 0.

- [ ] **Step 5: Report without staging or commit**

Report exact Git templates and every configuration-based denial.

### Task 10: Typed native and TypeScript gateways

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

**Interfaces:**
- Produces: list/decide/open-output process DTOs and discover/status/diff Git DTOs with strict normalizers.

- [ ] **Step 1: Write RED Rust/TS parity tests**

Prove invalid enum, unknown field, native path, raw grant, raw ID, oversized
output and stale generation are rejected before renderer state.

- [ ] **Step 2: Run RED focused tests**

Run:

```text
cargo test -p pillow-desktop --lib c4_gateway -- --nocapture
corepack pnpm --filter @pillow/desktop test -- c4-gateway-contract.test.ts
```

Expected: missing native modules and TS gateways make the focused tests fail.

- [ ] **Step 3: Implement typed commands and gateways**

Conversation capability is mandatory for process APIs; project capability is
mandatory for Git reads. DTOs contain portable paths and fixed public errors.

- [ ] **Step 4: Run GREEN, typecheck and native regression**

Run:

```text
cargo test -p pillow-desktop --lib
corepack pnpm --filter @pillow/desktop test -- c4-gateway-contract.test.ts
corepack pnpm --filter @pillow/desktop typecheck
corepack pnpm --filter @pillow/desktop build
cargo check --workspace --all-targets
git diff --check -- apps/desktop/src-tauri apps/desktop/src/lib apps/desktop/tests/c4-gateway-contract.test.ts
```

Expected: all commands exit 0.

- [ ] **Step 5: Report without staging or commit**

Report parity cases and privacy rejection evidence.

### Task 11: Mount C4.0 in the workbench

**Files:**
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/features/workbench/Workbench.tsx`
- Modify: `apps/desktop/src/features/workbench/ResourcePane.tsx`
- Modify: `apps/desktop/src/features/conversations/RunActivity.tsx`
- Modify: `apps/desktop/src/styles/workbench-conversation.css`
- Modify: `apps/desktop/src/styles/workbench-shell.css`
- Modify: `apps/desktop/src/styles/execution.css`
- Create: `apps/desktop/tests/c4-workbench-integration.test.tsx`

**Interfaces:**
- Consumes: Tasks 4 and 10.
- Produces: conversation-owned command summaries, one approval bar, contextual output and Git Changes modes.

- [ ] **Step 1: Write RED integration/race tests**

Cover selected-conversation ownership, project switch hard-clear, generation
discard, one approval bar, review/open/cancel behavior, no second dashboard,
1024 layout class and no native/raw metadata in DOM.

- [ ] **Step 2: Run RED**

Run:

`corepack pnpm --filter @pillow/desktop test -- c4-workbench-integration.test.tsx`

Expected: assertions for mounted process/Git surfaces fail.

- [ ] **Step 3: Implement minimal product wiring**

Reuse existing context pane and activity projection. Do not duplicate C3 write
bar authority; select the oldest pending action across disjoint write/process
domains with an explicit deterministic rule and never show two bars.

- [ ] **Step 4: Run GREEN and renderer aggregate**

Run:

```text
corepack pnpm --filter @pillow/desktop test -- c4-workbench-integration.test.tsx c4-execution-primitives.test.tsx c4-gateway-contract.test.ts
corepack pnpm --filter @pillow/desktop test
corepack pnpm --filter @pillow/desktop typecheck
corepack pnpm --filter @pillow/desktop build
git diff --check -- apps/desktop/src apps/desktop/tests
```

Expected: focused and aggregate renderer gates exit 0.

- [ ] **Step 5: Report without staging or commit**

Return screenshot paths only from deterministic acceptance and list race tests.

### Task 12: Real desktop C4.0 adversarial acceptance

**Files:**
- Create: `apps/desktop/tests/e2e/c4-governed-process-git.spec.ts`
- Create: `scripts/wave-c4-governed-process-git-acceptance.mjs`
- Modify: `apps/desktop/package.json`
- Create: `docs/verification/2026-08-02-wave-c4-acceptance.md` only after controller gates pass

**Interfaces:**
- Consumes: complete C4.0 product.
- Produces: terminal acceptance record, screenshots and residual-process proof.

- [ ] **Step 1: Add RED adversarial scenarios**

Required scenarios: reject, approve-success, nonzero-exit, expiry, cancel,
timeout, output truncation/lossy encoding, grandchild cleanup, two projects,
conversation switch, forced restart, Git status/diff/rename/binary/nested repo,
unsafe config denial and Git generation drift.

- [ ] **Step 2: Run focused RED/diagnose**

Run one serial C4 E2E. Classify failures as product, harness or host; do not
increase timeouts or weaken cleanup to turn red green.

- [ ] **Step 3: Implement only bounded harness corrections**

Harness edits may create acceptance fixtures and deterministic waits. Product
fixes return to their owning task allowlist and require a new worker brief.

- [ ] **Step 4: Controller full gates**

Run:

```text
corepack pnpm check
corepack pnpm --filter @pillow/desktop build
corepack pnpm desktop:test:e2e
cargo fmt --all --check
git diff --check
git diff --cached --name-only
```

Also inspect 1024x720 and 1440x960 screenshots, prove relevant residual
processes/listeners zero and record Critical 0 / Important 0.

- [ ] **Step 5: Controller acceptance only**

Only the controller writes acceptance/checkpoint status after rerunning every
required gate. No worker may declare C4 accepted, stage or commit.

## Execution rounds

### Round D1 — parallel foundations

- Grok1: Task 1 protocol.
- Grok2: Task 2 policy.
- Grok3: Task 4 isolated renderer primitives.
- Grok4: Task 3 Git parser foundation.

### Round D2 — high-risk execution and durable authority

- Real account-2 Codex CLI: Task 5 shared process executor.
- Grok1: Task 6 execution ledger, parallel with Task 5 after D1 acceptance.
- Grok4: Task 9 starts only after Tasks 3 and 5 are accepted.
- Grok2: Task 7 starts only after Tasks 1, 2, 5 and 6 are accepted.

### Round D3 — integration

- Grok1: Task 8 runtime-host coordination.
- Grok2: Task 10 native/TS gateways.
- Grok3: Task 11 workbench integration.
- Grok4: Task 12 harness foundation.
- Real account-2 Codex CLI: bounded high-risk repair only if D3 produces a
  protocol/process defect; never final review.

The controller reviews and reruns focused gates after every task, resolves
common-entry-file conflicts serially, runs final aggregate gates and owns the
only acceptance verdict.
