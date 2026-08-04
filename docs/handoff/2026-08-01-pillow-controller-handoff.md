# Pillow controller handoff

Snapshot: 2026-08-02 after C3 controller acceptance

Repository: `<workspace>\pillow`

Branch / HEAD: `main` / `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`

Handoff state: C1, C2 and C3 accepted working-tree checkpoints; C4 planning not started

Staging / commit: staging empty; no checkpoint commit has been authorized

## Executive summary

Pillow is being built in the locked order:

1. Stage 1: industrial local AI IDE.
2. Stage 2: Obsidian governance knowledge layer.
3. Stage 3: one-person-company project operating system.

Only Stage 1 is active. Its capability ladder is:

`C1 checkpoint -> C2 project resource workbench -> C3 governed write/diff/approval -> C4 terminal and Git -> D Codex/OpenCode runtime expansion -> E industrial Alpha`

C1, C2 and C3 are accepted working-tree checkpoints. C3 passed all eight real
Tauri/WebView2 adversarial scenarios, process/profile safety, the 246-test
renderer gate, Rust workspace/doctests and Clippy, the production UI build and
all 8 serial desktop E2E cases. C4 terminal and Git is the next planning
boundary only and must not start without a new explicit authorization.

The working tree is intentionally dirty and contains the accumulated C1, C2,
C3 and harness work. At this snapshot it has 258 porcelain entries: 75 modified
and 183 untracked; staging is empty. Do not reset, restore, overwrite, clean,
stage or commit it without explicit user authorization.

## Hard boundaries

- Work only in `<workspace>\pillow` for Pillow product code.
- `<workspace>\VaultSystem\keeper` is a frozen legacy resource manager. Do not put
  new IDE functionality there.
- Before editing, a new controller or worker must read `<workspace>\AGENTS.md`, this
  repository's `AGENTS.md`, and `<workspace>\VaultSystem\vault\AGENTS.md`, then boot
  through Keeper with `project=pillow` and its own task/role.
- Preserve the current dirty tree. Do not use destructive Git recovery.
- Do not stage or commit unless the user explicitly authorizes the exact
  checkpoint.
- C3 does not include shell execution, terminal, Git operations, network tools,
  Obsidian, key management, multiple runtimes, YOLO approval, or session-wide
  write permission.
- A proposal may perform one bounded UTF-8 create or modify only after explicit
  conversation-owned approval. Content, grant, native roots and raw internal
  identifiers must not leak into persistence, logs, DOM, screenshots or errors.
- The conversation stays the visual center. Project resources remain a
  collapsible context panel; agent activity uses progressive disclosure rather
  than a second dashboard.
- Do not create a periodic foreground monitor. Workers should send one
  completion/blocker signal; the controller performs acceptance after that
  signal.

## Required reading order

1. [`AGENTS.md`](../../AGENTS.md)
2. [`README.md`](../../README.md)
3. [`Pillow First design`](../superpowers/specs/2026-07-29-pillow-first-design.md)
4. [`Stage 1 plan comparison`](../roadmap/2026-07-30-stage1-plan-comparison.md)
5. [`Stage 1 overall status`](../roadmap/2026-07-31-stage1-overall-status.md)
6. [`C1 checkpoint`](../verification/2026-07-31-wave-c1-checkpoint.md)
7. [`C2 checkpoint`](../verification/2026-08-01-wave-c2-checkpoint.md)
8. [`C3 governed-write design`](../superpowers/specs/2026-08-01-c3-governed-write-design.md)
9. [`C3 implementation plan`](../superpowers/plans/2026-08-01-c3-governed-write-implementation.md)
10. [`C3-F integration report`](../execution/2026-08-01-wave-c3-integration-report.md)
11. [`C3-F project-isolation fix report`](../execution/2026-08-01-wave-c3f-project-isolation-fix-report.md)
12. [`Desktop process cleanup safety report`](../execution/2026-08-01-desktop-process-cleanup-safety-fix-report.md)
13. [`C3 controller acceptance`](../verification/2026-08-02-wave-c3-acceptance.md)
14. [`C3 checkpoint`](../verification/2026-08-02-wave-c3-checkpoint.md)

Also read the legacy capability inventory and upstream baseline before making
scope or provenance decisions:

- [`Legacy capability inventory`](../migration/legacy-capability-inventory.md)
- [`Upstream baseline`](../references/upstream-baseline.md)

## Repository truth at handoff

| Item | Current truth |
| --- | --- |
| Repository | `<workspace>\pillow` |
| Branch | `main` |
| HEAD | `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b` |
| Staging | Empty |
| Worktree | Dirty by design: 74 modified + 162 untracked at snapshot |
| Running Pillow/Cargo processes | None found at snapshot |
| Active workers | None; cleanup implementer and independent reviewer both completed |
| Last broad gate | `pnpm check` passed: 21 Vitest files / 246 tests plus Rust workspace tests, doctests and Clippy `-D warnings` |
| Other green gates | production UI build, `cargo fmt --all --check`, `git diff --check` |
| C3 desktop E2E | Pending; no fresh terminal record after the latest repairs |

`README.md` has already been corrected in the dirty tree: it no longer presents
the provider round trip as the next step. It records C1 as an accepted governed
provider-to-B3 read-only loop and C2 as an accepted checkpoint.

## Accepted capability state

### C1 — accepted checkpoint

C1 completes the governed provider-to-B3 read-only loop. Provider proposals
reach the governed read path and are projected into their owning conversation
timeline; unknown provider tools fail closed.

Primary evidence:

- [`C1 acceptance`](../verification/2026-07-31-wave-c1-acceptance.md)
- [`B3 read-execution acceptance`](../verification/2026-07-31-wave-b3-read-execution-acceptance.md)
- [`C1 checkpoint`](../verification/2026-07-31-wave-c1-checkpoint.md)

### C2 — accepted checkpoint

C2 supplies project-scoped file tree, search, recents, read-only text/Markdown/
image/basic-artifact preview, conversation-owned real agent activity,
project-switch isolation and restart recovery. It does not write files.

Primary evidence:

- [`C2 acceptance`](../verification/2026-08-01-wave-c2-acceptance.md)
- [`C2 checkpoint`](../verification/2026-08-01-wave-c2-checkpoint.md)
- [`C2 completion plan`](../superpowers/plans/2026-07-31-c2-read-only-workbench-completion.md)

### C3 — implemented through C3-F repairs, final gate pending

Implemented layers include:

- C3-A: strict protocol and policy contract.
- C3-B: durable metadata-only write-proposal ledger.
- C3-C: bounded diff and atomic host executor.
- C3-D: strict provider/runtime/native bridge.
- C3-E: conversation approval bar and bounded diff UI.
- C3-F: real desktop harness plus schema, cancellation, native-path privacy,
  provider cancel recovery and project-isolation repair passes.

The most recent product defect was a cross-project approval leak. The product
fix establishes a render-time ownership proof in `apps/desktop/src/App.tsx`.
Focused renderer tests passed 32/32, typecheck passed and the production build
passed. The harness was then corrected so that, after returning to Project A,
Project A's own restored approval bar is allowed while Project B evidence is
still forbidden (`allowOwnedApprovalBar: true`).

The subsequent focused real-desktop run was interrupted by the machine's forced
restart before a terminal completion record. This is neither a pass nor a
product failure. Treat C3-F as pending and resume from the focused acceptance
gate below.

## Forced-restart incident and repaired safety boundary

Windows recorded bugcheck `0xEF` (`CRITICAL_PROCESS_DIED`). The crash evidence
linked the prior harness cleanup path to `taskkill.exe` terminating a critical
`svchost.exe`. The unsafe design accumulated bare PIDs and used recursive forced
tree termination, allowing PID reuse to target an unrelated process.

The repository repair in `scripts/lib/desktop-process.mjs` now enforces:

- only a launcher-owned `ChildProcess` stable handle may terminate the root;
- descendants are identity-bound observation records and are never terminated;
- identity uses PID, executable path, executable name and exact FILETIME ticks;
- missing, incomplete, conflicting or never-observed identity fails closed;
- an incomplete observation followed by another complete identity without an
  observed absence remains uncertain and cannot be treated as safe PID reuse;
- residual proof is required before cleanup succeeds;
- Pillow harness code no longer invokes `taskkill.exe`, `/T`, or numeric
  SIGKILL/SIGTERM termination.

Independent review verdict: `PASS / APPROVED`, Critical 0, Important 0.
Controller evidence includes 29/29 focused safety tests, a controlled one-child
lifecycle probe, full `pnpm check`, production build, formatting/diff gates and
zero Pillow/fake-ACP/Cargo/rustc residual processes.

Do not reintroduce PID-tree killing, numeric forced termination, executable
name-wide cleanup, or any fallback to `taskkill.exe`. Do not work around a
fail-closed cleanup error by broadening termination authority.

One later Security log observation showed the Codex Desktop host itself
(`codex.exe`, outside Pillow/Node ownership) starting `taskkill.exe`. It did not
produce a system error, is outside this repository, and must not be attributed
to Pillow without parent/command/timestamp evidence. Prefer bounded foreground
commands and completion signals; avoid timeout-driven host cancellation when a
normal test completion can be awaited.

## Exact takeover procedure

Run the following from a fresh controller task. The boot response belongs to
the new controller; do not reuse the historical session identifier.

```powershell
Set-Location <workspace>\pillow

$body = @{
  project = "pillow"
  task = "pillow-controller-takeover"
  agentName = "<new-controller-name>"
  agentRole = "controller"
} | ConvertTo-Json

Invoke-RestMethod -Method Post `
  -Uri "http://127.0.0.1:7890/api/agent-bridge/boot" `
  -ContentType "application/json" `
  -Body $body

git rev-parse --show-toplevel
git branch --show-current
git rev-parse HEAD
git status --short
git diff --cached --name-only
```

Expected invariant before any edit:

- root is `<repo>`;
- branch is `main`;
- HEAD is still `7b7f70f...`, unless the user or another authorized controller
  can account for a later change;
- staging remains empty;
- the dirty tree remains present.

If these invariants differ, stop and reconcile the live state before editing.
Do not force it back to this snapshot.

## Next execution plan

### Gate 1 — re-establish process safety

Run the synthetic safety suite first:

```powershell
node --test scripts/lib/desktop-process-safety.test.mjs
```

Expected evidence: 29/29 pass. Then confirm no pre-existing Pillow,
`fake_acp_agent`, Cargo or rustc processes belong to this task. Do not kill
unknown or foreign processes.

### Gate 2 — focused C3-F real desktop acceptance

Run one serial foreground acceptance:

```powershell
node --test --test-concurrency=1 apps/desktop/tests/e2e/c3-governed-write.spec.ts
```

The suite timeout is 30 minutes and the orchestrator usually takes several
minutes. Let it finish normally. The terminal record must report:

- `status: passed`;
- `ipcBoundary: real-tauri-webview`;
- `processCleanup`, `normalClose` and `forcedRestartCleanup`: `passed`;
- all scenarios: reject-create, approve-create, approve-modify, base-drift,
  cancellation, conflict-replay, two-projects and forced-restart.

If it fails, preserve the terminal record, screenshots and process evidence.
First classify whether the failure is product, harness or host. Do not edit
product code merely to satisfy a stale assertion. In particular, Project A's
own bar is expected after switching back to A; foreign Project B evidence is
not.

### Gate 3 — broad controller acceptance

Only after Gate 2 passes, run:

```powershell
pnpm check
pnpm --filter @pillow/desktop build
cargo fmt --all --check
git diff --check
git diff --cached --name-only
```

Also verify normal close, no residual Pillow/fake-ACP/Cargo/rustc processes,
and no native root, grant, raw session/run/tool identifier or provider-controlled
raw output in product DOM/log/screenshot evidence.

### Gate 4 — controller final review

Grok4 and account-2 Codex CLI may own bounded development tasks, but neither is
the final reviewer. The Codex Desktop controller inspects every changed line,
classifies Critical/Important issues and reruns focused plus full gates.

### Gate 5 — C3 checkpoint decision

Gates 1-4 are green and C3 acceptance/checkpoint plus Stage 1 status are now
recorded. Still do not stage or commit until the user explicitly authorizes it.

Only after the C3 checkpoint is accepted may the controller plan C4 terminal
and Git. C4 implementation is a separate authorization boundary.

## Worker routing contract

The controller owns scope, protocol, file allowlists, integration decisions,
acceptance and Keeper completion. Delegate implementation to workers with
non-overlapping write paths; do not make two workers edit the same file.

Every worker brief must include:

- a fresh Keeper boot and lease only if a runtime slot is actually required;
- exact input/output contract;
- explicit file write allowlist and forbidden paths;
- exact focused and broad test commands;
- no stage/commit;
- completion payload: changed files, tests, remaining risks, staging state,
  residual process state and a completion/blocker signal.

Do not dispatch audit-only workers before the implementation gates need them.
Use an independent reviewer only after the development tree is green. Release
worker processes and leases immediately on completion.

## Provenance constraints for later UI work

The local references `<workspace>\zbbody` and
`<workspace>\VaultSystem\references\grok-app` were used for interaction research.
They are references, not product code sources by default.

- Grok App direct adaptation requires the locked upstream commit and license
  provenance to be recorded.
- Verify zbbody's license before any direct reuse; otherwise keep it as a
  conceptual reference.
- Reuse the interaction principles—conversation-owned activity, expandable
  steps, role state and progressive output disclosure—without copying a
  dashboard/card stack or exposing raw debug metadata.

Relevant research:

- [`Grok App interaction research`](../research/2026-08-01-c3-grok-app-interaction-research.md)
- [`zbbody interaction research`](../research/2026-08-01-c3-zbbody-interaction-research.md)
- [`C3 security research`](../research/2026-08-01-c3-governed-write-security-research.md)

## Handoff completion state

- No workers remain in flight.
- C3 is controller-accepted; evidence is in the 2026-08-02 acceptance and
  checkpoint documents.
- No file was staged or committed.
- Relevant Pillow/fake-ACP/Cargo/rustc residuals are zero. Seven generic Node
  interval processes exist without safe handles; two came from interrupted
  account-2 CLI RED/refinement runs and five predated that task. Do not kill
  them numerically without a separate authorized cleanup procedure.
- The next controller begins with C4 research/design only after user approval.
- The handoff does not authorize C4 implementation, staging, commit, packaging
  or publishing.
