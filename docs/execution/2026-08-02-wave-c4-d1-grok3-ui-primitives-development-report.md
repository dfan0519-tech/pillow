# C4-D1 Grok3 development report — isolated execution UI primitives

Owner: grok3 (implementation worker)
Role: real Grok CLI slot `grok3`
Keeper worker session: `<keeper-session>`
Keeper parent session: `<keeper-session>`
Date: 2026-08-02
Task: `c4-d1-execution-ui-primitives` (C4 implementation plan Task 4)

## Scope

Build controlled, unmounted C4 UI primitives with TDD:

- command activity row
- approve-once command approval bar
- bounded terminal-output view
- Git Changes view

Components receive sanitized DTOs and callbacks only. They must not import
Tauri, fetch, persist, infer terminal truth, modify selected project, or expose
native paths, grants, or raw identifiers.

## Delivered

| Path | Role |
| --- | --- |
| `apps/desktop/src/features/execution/executionTypes.ts` | `CommandOrigin`, `CommandState`, proposal/activity/terminal/Git DTOs, `formatArgvDisplay` |
| `apps/desktop/src/features/execution/CommandActivityRow.tsx` | Compact timeline row + progressive disclosure |
| `apps/desktop/src/features/execution/CommandApprovalBar.tsx` | Approve once · Reject · Review command |
| `apps/desktop/src/features/execution/TerminalOutputView.tsx` | Bounded stdout/stderr surface |
| `apps/desktop/src/features/execution/GitChangesView.tsx` | Status list + on-demand host diff body |
| `apps/desktop/src/styles/execution.css` | Isolated styles (feature-import only; not product-entry) |
| `apps/desktop/tests/c4-execution-primitives.test.tsx` | RED/GREEN interaction + privacy gates |
| `docs/execution/2026-08-02-wave-c4-d1-grok3-ui-primitives-development-report.md` | This report |

### Types (Task 4 exact unions)

```ts
export type CommandOrigin = "agent" | "user";
export type CommandState =
  | "pending_approval" | "approved" | "running" | "succeeded"
  | "failed" | "rejected" | "expired" | "cancelled" | "interrupted";
```

View models produced for later gateway/workbench wiring:

- `CommandProposalView`
- `CommandActivityView`
- `TerminalOutputModel` (+ `TerminalOutputChunk`)
- `GitStatusView` / `GitStatusEntry` / `GitDiffView`

### Interaction contracts

| Surface | Behavior |
| --- | --- |
| Activity row | One compact row: origin chip · status · program · literal argv · exit/duration; nested tail closed by default; optional “完整输出” callback |
| Approval bar | Program + literal argv + portable cwd + risk; grant in `grantRef` only; initial focus Approve; Tab cycles actions; Escape rejects only while bar owns focus |
| Terminal output | Sequence-sorted chunks; truncation + lossy labels; byte meta; not a shell |
| Git Changes | Branch/HEAD/dirty count; rename as one entry (old+new); binary without invented unified text; stale notice + refresh; path select callback |

### Privacy / boundary

- No `@tauri-apps`, `fetch`, `invoke`, or storage APIs in feature modules (source scan + test).
- DOM/HTML assertions reject proposalId, conversationId, runId, grant, and native path shapes.
- Identifiers may exist on DTOs for parent keying; components never paint them.
- `execution.css` is imported by feature modules only. `main.tsx` / `App.tsx` /
  `screenshot-main.tsx` were not modified. Components are unmounted.

## TDD evidence

### RED

Before feature sources existed, focused import of
`c4-execution-primitives.test.tsx` would fail module resolution. Tests were
authored first against the Task 4 interface, then components implemented to
GREEN.

### GREEN

```text
corepack pnpm --filter @pillow/desktop test -- c4-execution-primitives.test.tsx
corepack pnpm --filter @pillow/desktop typecheck
corepack pnpm --filter @pillow/desktop build
git diff --check -- apps/desktop/src/features/execution \
  apps/desktop/src/styles/execution.css \
  apps/desktop/tests/c4-execution-primitives.test.tsx
```

| Gate | Result |
| --- | --- |
| Focused file | **18/18** passed (`tests/c4-execution-primitives.test.tsx`) |
| Desktop vitest aggregate (filter arg still ran full suite via vitest) | **22 files / 264 tests** passed |
| Typecheck | exit 0 |
| Production UI build | exit 0 (`vite build`, ~1.16s) |
| `git diff --check` (allowlist) | exit 0 |
| Product mount | none (`main.tsx` / `App.tsx` / global CSS entry unchanged) |
| Staging | empty |

### Accessibility / keyboard

- Approval bar: Approve focus on mount; Tab / Shift+Tab cycle approve → reject → review; Escape rejects only when focus is inside the bar.
- Nested activity tail and terminal/diff bodies are keyboard-focusable (`tabIndex={0}`) for copy of already-sanitized text.
- Progressive disclosure uses native `<details>` / `<summary>` (Enter/Space).

### Privacy checks

- Explicit `expectNoSecrets` helper on activity, approval, terminal, and Git renders.
- Feature source scan forbids Tauri/fetch/storage/invoke.
- Grant held in ref only (mirrors C3 `WriteApprovalBar`); never in React state/DOM attributes.

## Explicit non-goals (this task)

- No Workbench / ResourcePane / RunActivity mount (Task 11).
- No Tauri gateway or native commands (Task 10).
- No process executor, ledger, policy, or Git parser work (other D1 slots).
- No stage, commit, package, or publish.

## Risks / residual

1. **Unmounted** — product still has no process/Git UI until Task 11 wiring and
   Task 10 gateways exist.
2. **DTO field names** — Task 10 normalizers must adopt these shapes (or map
   explicitly); plan listed `CommandProposalView` / `CommandActivityView` /
   `GitStatusView` / `GitDiffView`.
3. **Write vs process bar dual domain** — Task 11 must pick one pending bar
   across write and process domains; this task only ships the process bar primitive.
4. **Output virtualization** — Terminal view renders provided chunks only; large
   logs still need host cap + later virtualization gate (research T20).
5. **Vitest filter** — pnpm/vitest currently executes the full desktop suite when
   `-- c4-execution-primitives.test.tsx` is passed; focused file still green
   inside the aggregate.

## Staging / process

- Staging empty; no commit.
- No desktop E2E or real Tauri process started for this slot.
- Pillow residual processes not launched by this worker.

## Completion

**Status: DONE**

Worker complete for Task 4 isolated renderer primitives. Not controller-accepted.
