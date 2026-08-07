# C4 D3B4 Git Changes Implementation Plan

> Controller rule: do not start this plan until D3B3 `pnpm check`, production
> build, full serial desktop E2E and residual-process gates are green.

**Goal:** Mount the accepted read-only Git executor as a real Tauri-backed
Changes mode in the existing contextual resource pane and prove it in a real
Windows desktop run.

**Architecture:** A new native `GitCommandState` owns per-project discovery
generation and composes `GitReadExecutor`. The renderer uses the existing
strict `GitGateway`; `App` guards project/generation state and the existing
resource pane progressively discloses repository, status and one diff.

**Worker policy:** Grok workers do implementation. Each worker boots Keeper
only for record, writes only its explicit allowlist, does not stage/commit, and
ends with changed files, tests, residual risk and a completion line. The
controller reviews source and executes all acceptance gates.

---

## Round 1 — Native and typed composition

Round 1 has two parallel, non-overlapping implementation tasks.

### Task 1A — Grok1: native Tauri Git authority

**Write allowlist**

- Create: `apps/desktop/src-tauri/src/git_commands.rs`
- Create: `apps/desktop/src-tauri/src/git_commands_tests.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Create: `docs/execution/2026-08-05-wave-c4-d3b4-grok1-native-git-report.md`

**Contract**

Implement `GitCommandState` and the exact Tauri commands
`discover_git_repositories`, `get_git_status`, and `open_git_diff`. Resolve the
project through `AppState`; never accept roots, executables or argv from the
renderer. Keep per-project latest discovery/generation, validate repository
membership and availability, call only `GitReadExecutor`, and map errors to the
accepted public codes without paths.

Tests must cover registered/missing projects, generation stale/mismatch,
unavailable/missing repositories, portable paths, host Git unavailable,
nested repository ids, path-free errors and command registration.

**Focused gates**

```powershell
node scripts/run-cargo.mjs test -p pillow-desktop git_commands -- --nocapture
node scripts/run-cargo.mjs clippy -p pillow-desktop --all-targets -- -D warnings
node scripts/run-cargo.mjs fmt --all -- --check
git diff --check -- apps/desktop/src-tauri/src/git_commands.rs apps/desktop/src-tauri/src/git_commands_tests.rs apps/desktop/src-tauri/src/lib.rs apps/desktop/src-tauri/Cargo.toml docs/execution/2026-08-05-wave-c4-d3b4-grok1-native-git-report.md
```

### Task 1B — Grok2: renderer bridge convergence

**Write allowlist**

- Modify: `apps/desktop/src/lib/gitGateway.ts`
- Modify: `apps/desktop/src/lib/tauriGitGateway.ts`
- Create: `apps/desktop/tests/tauri-git-gateway.test.ts`
- Create: `docs/execution/2026-08-05-wave-c4-d3b4-grok2-git-bridge-report.md`

**Contract**

Keep `gitGateway.ts` as the single wire DTO source. Verify the invoke names and
arguments against Task 1A, retain strict allowlisted fields and bounds, reject
malformed, overlong, native, traversal and generation-drift responses, and map
only the public error vocabulary. Do not add UI state or product mounting.

**Focused gates**

```powershell
pnpm --filter @pillow/desktop test -- tests/tauri-git-gateway.test.ts
pnpm --filter @pillow/desktop typecheck
git diff --check -- apps/desktop/src/lib/gitGateway.ts apps/desktop/src/lib/tauriGitGateway.ts apps/desktop/tests/tauri-git-gateway.test.ts docs/execution/2026-08-05-wave-c4-d3b4-grok2-git-bridge-report.md
```

### Controller integration gate 1

Review the native/TS contract field by field. Run both focused suites together,
desktop typecheck, scoped Clippy/fmt/diff, and confirm staging is empty. Reject
Round 1 on any root/executable/argv leak or duplicate wire DTO.

---

## Round 2 — Contextual Changes workbench

Round 2 starts only after integration gate 1 is green. Two workers use a
predeclared prop contract and non-overlapping paths.

### Task 2A — Grok3: Changes pane view and DTO convergence

**Write allowlist**

- Modify: `apps/desktop/src/features/execution/GitChangesView.tsx`
- Modify: `apps/desktop/src/features/execution/executionTypes.ts`
- Modify: `apps/desktop/src/features/workbench/ResourcePane.tsx`
- Modify: `apps/desktop/src/styles/execution.css`
- Modify: `apps/desktop/tests/c4-execution-primitives.test.tsx`
- Modify: `apps/desktop/tests/resource-pane.test.tsx`
- Create: `docs/execution/2026-08-05-wave-c4-d3b4-grok3-git-pane-report.md`

**Contract**

Consume wire DTOs from `gitGateway.ts`, remove duplicate Git wire types, and
mount a compact Changes body in `ResourcePane`. Add only view props for loading,
path-free public error, repository selection, selected path, refresh and diff
selection. Preserve the conversation-centred layout and existing Files,
Preview, Diff and Terminal modes. Do not create gateway calls or own project
state in this task.

**Focused gates**

```powershell
pnpm --filter @pillow/desktop test -- tests/c4-execution-primitives.test.tsx tests/resource-pane.test.tsx
pnpm --filter @pillow/desktop typecheck
git diff --check -- apps/desktop/src/features/execution/GitChangesView.tsx apps/desktop/src/features/execution/executionTypes.ts apps/desktop/src/features/workbench/ResourcePane.tsx apps/desktop/src/styles/execution.css apps/desktop/tests/c4-execution-primitives.test.tsx apps/desktop/tests/resource-pane.test.tsx docs/execution/2026-08-05-wave-c4-d3b4-grok3-git-pane-report.md
```

### Task 2B — Grok4: App/Workbench Git state composition

**Write allowlist**

- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/main.tsx`
- Modify: `apps/desktop/src/features/workbench/Workbench.tsx`
- Create: `apps/desktop/tests/c4-git-changes-integration.test.tsx`
- Create: `docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-report.md`

**Contract**

Construct `TauriGitGateway` in `main.tsx`. In `App`, own discovery/status/diff
loading and public error state, clear it synchronously on project switch, and
guard every async result by project id and generation. In `Workbench`, expose
the existing contextual Changes mode and pass the agreed view contract to
`ResourcePane`. No permanent dashboard, polling loop, raw id, native path,
mutation or terminal command.

Tests must prove project switch isolation, late-result dropping, refresh
generation invalidation, nested repository selection, stale recovery, one-path
diff selection and no gateway call when no project is selected.

**Focused gates**

```powershell
pnpm --filter @pillow/desktop test -- tests/c4-git-changes-integration.test.tsx
pnpm --filter @pillow/desktop typecheck
git diff --check -- apps/desktop/src/App.tsx apps/desktop/src/main.tsx apps/desktop/src/features/workbench/Workbench.tsx apps/desktop/tests/c4-git-changes-integration.test.tsx docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-report.md
```

### Controller integration gate 2

Review all state transitions and visually inspect the supported Windows-size
production build. Run the combined bridge/pane/workbench tests and typecheck.
Reject stale cross-project rendering, duplicate dashboards, wire/view DTO
mixing or invented binary/rename data.

---

## Round 3 — Real desktop Git acceptance

### Task 3 — Grok4: Tauri/WebView2 Git E2E

**Write allowlist**

- Create: `scripts/lib/c4-git-changes-acceptance.mjs`
- Create: `scripts/wave-c4-git-changes-acceptance.mjs`
- Create: `apps/desktop/tests/e2e/c4-git-changes.spec.ts`
- Modify: `apps/desktop/package.json`
- Create: `docs/execution/2026-08-05-wave-c4-d3b4-grok4-real-desktop-git-e2e-report.md`

**Contract**

Use the production Tauri composition and real Git executable against isolated
temporary project fixtures. Prove project-root and nested-repository discovery,
ordinary/untracked/rename/conflict status, detached HEAD, one bounded text
diff, binary/oversized summary, unsafe configuration denial, generation drift,
project switch isolation and restart refresh. Assert the fixture working tree,
index, HEAD and configuration hashes are unchanged after read operations.

The harness must reuse the accepted launcher-owned process supervision. It must
not kill by numeric PID or process name and must finish with zero Pillow
processes and zero port 1420 listeners.

**Focused gates**

```powershell
node --check scripts/lib/c4-git-changes-acceptance.mjs
node --check scripts/wave-c4-git-changes-acceptance.mjs
pnpm --filter @pillow/desktop test:e2e:c4:git
git diff --check -- scripts/lib/c4-git-changes-acceptance.mjs scripts/wave-c4-git-changes-acceptance.mjs apps/desktop/tests/e2e/c4-git-changes.spec.ts apps/desktop/package.json docs/execution/2026-08-05-wave-c4-d3b4-grok4-real-desktop-git-e2e-report.md
```

---

## Round 4 — Controller acceptance and checkpoint

The controller personally performs:

1. complete changed-file source review against all allowlists;
2. focused native, bridge, pane, workbench and real desktop Git tests;
3. `pnpm check`;
4. `pnpm --filter @pillow/desktop build`;
5. `pnpm --filter @pillow/desktop test:e2e` in serial;
6. `node scripts/run-cargo.mjs fmt --all -- --check`;
7. `git diff --check` and staging-empty verification;
8. fresh process/listener residual inspection;
9. supported-size screenshot review for conversation dominance and compact
   progressive disclosure;
10. a C4 checkpoint update recording D3B3 process and D3B4 read-only Git truth.

No staging or commit occurs without explicit user authorization.
