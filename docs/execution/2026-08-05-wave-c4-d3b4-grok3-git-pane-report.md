# C4 D3B4 Round 2A — Grok3 Git Changes pane report

Date: 2026-08-05  
Worker: grok3 (Grok CLI implementation worker)  
Task: `c4-d3b4-git-pane`  
Brief: `docs/execution/2026-08-05-wave-c4-d3b4-grok3-git-pane-brief.md`  
Design: `docs/superpowers/specs/2026-08-05-c4-git-changes-design.md`

## Keeper

- Attempted worker boot (`project: pillow`, `task: c4-d3b4-git-pane`,
  `agentName: grok3`, `agentRole: worker`) → HTTP 400 Bad Request (worker claims
  require an unavailable parent session id; no parent supplied by dispatcher).
- Did not impersonate the controller role. No completion post was attempted.
- Keeper used for record attempt only; no execution, judgment, Vault query, or
  prompt rewrite.

## Delivered

Converged the isolated Git Changes primitive onto authoritative gateway wire
DTOs and mounted it as one compact mode of the existing contextual
`ResourcePane`. This slice is view-only: no gateway calls, no project Git
state, no polling, no mutation.

### Changed files (allowlist only)

| Path | Change |
| --- | --- |
| `apps/desktop/src/features/execution/executionTypes.ts` | Removed duplicate Git wire DTO declarations (`GitStatusKind` / entry / status / diff). Process and terminal types retained. |
| `apps/desktop/src/features/execution/GitChangesView.tsx` | Consumes `gitGateway` DTOs; UI-only `loading` / `error` / `stale`; multi-repo selector; empty/clean/detached states. |
| `apps/desktop/src/features/workbench/ResourcePane.tsx` | Exported exact `GitChangesPaneModel`; optional `gitChanges` prop; mode priority Terminal → write Diff → Changes → Files/Preview. |
| `apps/desktop/src/styles/execution.css` | Repo selector, error/status copy styles; removed invented binary-badge entry chrome. |
| `apps/desktop/tests/c4-execution-primitives.test.tsx` | Gateway-shaped fixtures; binary/unsupported/stale/repo-selector coverage; source boundary against wire-field invention. |
| `apps/desktop/tests/resource-pane.test.tsx` | Workbench pane mount, priority, callbacks, Files/Preview preservation when `gitChanges` absent. |
| `docs/execution/2026-08-05-wave-c4-d3b4-grok3-git-pane-report.md` | This report. |

No other paths were modified by this worker.

## Interaction decisions

1. **Single wire source** — `GitChangesView` imports `GitStatusView`,
   `GitDiffView`, `GitRepositoryView`, and related kinds only from
   `src/lib/gitGateway.ts`. `executionTypes.ts` no longer redeclares Git wire
   records.
2. **UI-only presentation flags** — `loading`, path-free `error`, and `stale`
   are separate props / `GitChangesPaneModel` fields. They are never added to
   wire status or diff records. No `isBinary`, `stale`, or `unavailableReason`
   on gateway entries.
3. **Binary truth** — Binary and unsupported appear only from `diff.kind`.
   Status rows never invent a binary badge.
4. **Repository selector** — Rendered only when `repositories.length > 1`.
   Unavailable repositories are visible and disabled; available ones call
   `onSelectRepository` with the portable id only.
5. **Mode priority** — For an open pane with an active project:
   Terminal output → governed-write Diff → Changes (`gitChanges` present) →
   ordinary resource browser/preview/fallback. Conversation remains the
   dominant surface; Changes is compact body content only.
6. **Callbacks only** — Refresh, path select, and repository select invoke the
   supplied model callbacks with portable ids/paths. No native paths, argv,
   session, executor, or debug metadata. No polling.
7. **Round 2B contract** — Exported exactly:

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

Grok4 will own App/Workbench discovery, generation guards, and gateway calls.

## Gates

### Focused Vitest

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/c4-execution-primitives.test.tsx tests/resource-pane.test.tsx
```

- **PASS**: 2 files, **43 tests** (28 + 15), exit 0.
  (First run hit a transient vitest temp-cache ENOENT; re-run with
  `--no-cache` then clean re-run both green.)

### Typecheck

```powershell
pnpm --filter @pillow/desktop typecheck
```

- **PASS**: `tsc --noEmit`, exit 0.

### Diff check

```powershell
git diff --check -- apps/desktop/src/features/execution/GitChangesView.tsx apps/desktop/src/features/execution/executionTypes.ts apps/desktop/src/features/workbench/ResourcePane.tsx apps/desktop/src/styles/execution.css apps/desktop/tests/c4-execution-primitives.test.tsx apps/desktop/tests/resource-pane.test.tsx docs/execution/2026-08-05-wave-c4-d3b4-grok3-git-pane-report.md
```

- **PASS**: exit 0 (CRLF normalization notices only; no whitespace errors).

Desktop E2E was not run (brief forbids).

## Coverage proven by tests

- Branch identity, dirty count, portable paths, rename source, detached HEAD,
  clean workspace
- Text / binary / unsupported diffs without invented unified text
- UI-only stale + path-free error + loading + refresh/path callbacks
- Multi-repo selector; unavailable repo not selectable; single-repo hides selector
- Source boundary: no Git wire types left in `executionTypes.ts`; no
  `isBinary` / `status.stale` / `unavailableReason` in `GitChangesView`
- Workbench pane mounts Changes when `gitChanges` present; heading “变更”
- Terminal and write Diff still outrank Changes
- Files/Preview path preserved when `gitChanges` is null and resource gateway
  is present
- Existing C2-C1 resource browser suite still green in the same file

## Residual risk

- Round 2B (Grok4) must still own discovery/status/diff loading, project and
  generation isolation, and late-result drops. This pane will render whatever
  model the host supplies; it cannot itself prevent host-side merge of stale
  discovery.
- Workbench `ResourcePane` Git mode tests live alongside the C2-C1 project
  browser suite in `resource-pane.test.tsx` per allowlist (two components,
  one file). Controllers may later split if desired.
- Product mount through App/Workbench is intentionally out of this allowlist.
- Full desktop suite and real Tauri Git fixtures are controller/Round 2B/3
  gates, not this worker’s scope.

## Tree hygiene

- HEAD not advanced; no stage, commit, reset, clean, restore, or unrelated
  reformat.
- Only allowlisted Git pane paths were edited/created by this worker.
- Pre-existing dirty/untracked work elsewhere in the tree was left untouched.
- Staging remains empty from this worker.

## Completion

C4 D3B4 Round 2A Git Changes pane delivered for controller review.

`C4-D3B4-GROK3-GIT-PANE-COMPLETE`
