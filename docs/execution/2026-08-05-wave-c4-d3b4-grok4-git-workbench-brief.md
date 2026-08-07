# C4 D3B4 Round 2B — Grok4 Git workbench composition brief

Date: 2026-08-05  
Implementer: actual Grok CLI slot `grok4`  
Controller acceptance: Codex Desktop, independent source review and fresh gates

## Read first

1. `<repo>\AGENTS.md`
2. `<repo>\docs\superpowers\specs\2026-08-05-c4-git-changes-design.md`
3. `<repo>\docs\superpowers\plans\2026-08-05-c4-git-changes-implementation.md`
4. `<repo>\docs\execution\2026-08-05-wave-c4-d3b4-grok3-git-pane-report.md`
5. the current versions of every allowlisted product and test file below

Round 1 native/typed Git composition and Round 2A contextual pane are already
controller-accepted. This task owns only the renderer integration between the
strict `GitGateway`, `App`, `Workbench`, and the existing `ResourcePane` model.

Keeper is record-only. Attempt worker boot once. If the endpoint rejects a
worker without a dispatcher parent session id, record that result in the report
and continue; do not boot as controller and do not use Keeper for execution or
judgment.

## Exact write allowlist

- Modify `apps/desktop/src/App.tsx`
- Modify `apps/desktop/src/main.tsx`
- Modify `apps/desktop/src/features/workbench/Workbench.tsx`
- Create `apps/desktop/tests/c4-git-changes-integration.test.tsx`
- Create `docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-report.md`

Do not edit any other file. In particular, do not alter the already accepted
Rust commands, Git gateways, `GitChangesView`, `ResourcePane`, styles, package
scripts, E2E harnesses, plans, lockfiles, or unrelated dirty work. Do not stage,
commit, reset, clean, restore, reformat unrelated code, or launch desktop E2E.

## Authoritative interfaces

- `GitGateway` and every Git wire DTO come only from
  `apps/desktop/src/lib/gitGateway.ts`.
- `main.tsx` constructs exactly one `TauriGitGateway` and supplies it to `App`.
- `App` receives an optional `gitGateway?: GitGateway` and owns the complete
  discovery/status/diff presentation state for the active project.
- `Workbench` receives an optional `gitChanges?: GitChangesPaneModel | null`
  and passes that model unchanged to `ResourcePane`.
- Import `GitChangesPaneModel` from
  `apps/desktop/src/features/workbench/ResourcePane.tsx`; do not redeclare it.

The accepted model is exactly:

```ts
export type GitChangesPaneModel = {
  repositories: GitRepositoryView[];
  selectedRepositoryId: string | null;
  status: GitStatusView | null;
  diff: GitDiffView | null;
  loading: boolean;
  error: string | null;
  stale: boolean;
  onSelectRepository: (repositoryId: string) => void;
  onSelectPath: (portablePath: string) => void;
  onRefresh: () => void;
};
```

## Required behavior

Implement with focused RED/GREEN tests before finalizing product code.

1. With no active project or no `gitGateway`, do not call Git and pass
   `gitChanges={null}`. Existing resource, write-diff, and terminal behavior
   must remain unchanged.
2. On active-project selection, discover repositories once, select the first
   available repository deterministically, and load its status with the exact
   discovery generation. If none is available, keep a compact path-free public
   error state and do not call status or diff.
3. On every project switch, clear repositories, selection, status, diff,
   loading/error/stale synchronously before any new result may render.
4. Guard every discovery, status, and diff fulfillment/rejection by both the
   project id captured at start and a controller-owned monotonically increasing
   request/scope generation. A late result from the prior project, repository,
   refresh, or selected path must be dropped without mutating visible state.
5. Repository selection accepts only an available repository id from the
   current discovery result, clears the current diff/error/stale state, and
   loads status using the same current discovery generation.
6. Path selection accepts only a portable path present in the current status,
   clears the previous diff, and calls `openGitDiff` with the current project,
   selected repository, discovery generation, and that path. Never derive or
   expose a native path.
7. Refresh invalidates all prior requests, clears the selected diff, re-runs
   discovery, and uses only the new returned generation. A stale/mismatch error
   sets `stale=true` and the normalized path-free public message. Other errors
   remain path-free and set `stale=false`.
8. There is no timer, interval, polling loop, dashboard, terminal command,
   mutation, raw session/debug id, root, executable, argv, cwd, environment, or
   host error echo.
9. Keep the conversation surface visually dominant. The existing pane toggle
   opens the contextual pane; Git is merely another pane mode with the accepted
   precedence owned by `ResourcePane`.

## Required tests

Create focused integration tests covering at least:

- no selected project means zero gateway calls and `gitChanges=null`;
- project switch clears old Git content before new promises settle;
- late discovery/status/diff results from an old project or invalidated refresh
  are dropped;
- refresh adopts only the new discovery generation;
- nested available repository selection and unavailable-repository rejection;
- one valid current-status portable path opens one diff with the exact
  project/repository/generation tuple;
- stale/mismatch error recovery is path-free and refreshable;
- existing ResourcePane toggle and Terminal/write Diff precedence remain
  intact through the supplied model.

Use controllable deferred promises; do not use sleeps or polling in tests.

## Worker gates

Run serially:

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/c4-git-changes-integration.test.tsx --no-cache
pnpm --filter @pillow/desktop typecheck
git diff --check -- apps/desktop/src/App.tsx apps/desktop/src/main.tsx apps/desktop/src/features/workbench/Workbench.tsx apps/desktop/tests/c4-git-changes-integration.test.tsx docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-report.md
git diff --cached --name-only
```

## Report and completion contract

Write the report at the exact allowlisted path. Include:

- worker identity and Keeper boot result;
- changed files;
- state/authority decisions;
- exact commands, exit codes, and test counts;
- self-review against every required behavior above;
- residual risks for Round 3 real desktop Git E2E;
- confirmation that staging is empty and unrelated dirty work was preserved.

End the report and final stdout result with:

`C4-D3B4-GROK4-GIT-WORKBENCH-COMPLETE`

The controller will independently review source and run focused/aggregate
acceptance. Worker completion is not acceptance.
