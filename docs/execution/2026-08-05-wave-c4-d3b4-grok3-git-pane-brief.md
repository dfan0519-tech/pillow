# C4 D3B4 Round 2A — Grok3 Git Changes pane brief

You are the real Grok CLI `grok3` implementation worker. Converge the existing
Git Changes primitive onto the accepted gateway DTOs and mount it as one compact
mode of the existing contextual `ResourcePane`. Do not call gateways or own
project state.

## Startup

1. Work only in `<repo>`.
2. Read `AGENTS.md`, the accepted Git Changes design, this brief, and the
   current `GitChangesView.tsx` / `ResourcePane.tsx` before editing.
3. Boot Keeper once for record only: project `pillow`, task
   `c4-d3b4-git-pane`, agentName `grok3`. If worker boot requires an unavailable
   parent id, record the failure in the report and continue; do not impersonate
   the controller role. Keeper must not execute or judge project work.
4. Preserve the dirty tree. No stage, commit, reset, clean, restore, or unrelated
   reformat.

## Write allowlist

- Modify `apps/desktop/src/features/execution/GitChangesView.tsx`
- Modify `apps/desktop/src/features/execution/executionTypes.ts`
- Modify `apps/desktop/src/features/workbench/ResourcePane.tsx`
- Modify `apps/desktop/src/styles/execution.css`
- Modify `apps/desktop/tests/c4-execution-primitives.test.tsx`
- Modify `apps/desktop/tests/resource-pane.test.tsx`
- Create `docs/execution/2026-08-05-wave-c4-d3b4-grok3-git-pane-report.md`

No other path may change.

## Exact shared contract for Round 2B

Export this view-only model from `ResourcePane.tsx` (field names may not drift):

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

Add `gitChanges?: GitChangesPaneModel | null` to `ResourcePaneProps`. Grok4 will
later pass this exact prop. UI-only loading/error/stale remain in the model;
wire DTOs come only from `src/lib/gitGateway.ts`.

## Required behavior

- Remove duplicate Git wire DTO declarations from `executionTypes.ts` and make
  `GitChangesView` consume gateway DTOs.
- Do not invent `isBinary`, stale or unavailable fields on wire records.
- When `gitChanges` is present for the active project, show a compact Changes
  mode ahead of ordinary resource fallback but behind explicit Terminal and
  governed-write Diff targets.
- Render a repository selector only for multiple repositories. Unavailable
  repositories are visible but not selectable as active reads.
- Preserve branch/detached HEAD, dirty count, rename source, status kinds,
  empty/loading/error/stale/truncated states and text/binary/unsupported diff.
- Refresh and file/repository clicks call only the supplied callbacks with
  portable ids/paths. No polling and no native/debug metadata.
- Keep conversation visually dominant; no permanent cards, background task
  table or second dashboard.
- Preserve all existing Files/Preview/Diff/Terminal behavior and tests.

## Gates

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/c4-execution-primitives.test.tsx tests/resource-pane.test.tsx
pnpm --filter @pillow/desktop typecheck
git diff --check -- apps/desktop/src/features/execution/GitChangesView.tsx apps/desktop/src/features/execution/executionTypes.ts apps/desktop/src/features/workbench/ResourcePane.tsx apps/desktop/src/styles/execution.css apps/desktop/tests/c4-execution-primitives.test.tsx apps/desktop/tests/resource-pane.test.tsx docs/execution/2026-08-05-wave-c4-d3b4-grok3-git-pane-report.md
```

Do not run desktop E2E. Report changed files, interaction decisions, tests,
risks and tree hygiene. End with:

`C4-D3B4-GROK3-GIT-PANE-COMPLETE`

