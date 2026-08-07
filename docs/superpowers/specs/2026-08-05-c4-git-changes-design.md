# C4 D3B4 Git Changes Design

Date: 2026-08-05  
Status: accepted for implementation after D3B3 controller gates are green

## Outcome

Complete the remaining C4.0 read-only Git slice inside Pillow's existing
conversation-centred workbench. A user can open the contextual resource pane,
discover repositories below the selected project, inspect branch/HEAD and a
bounded status list, and open one bounded diff. This slice does not stage,
commit, checkout, reset, discard, mutate configuration, or expose a terminal.

## Product placement

Git Changes is one collapsible mode of the existing resource pane. It is not a
second dashboard and does not compete with the conversation timeline.

The disclosure sequence is:

1. a compact Changes entry showing the active repository and dirty count;
2. a repository/status list inside the contextual pane;
3. one selected file's text, binary, oversized, or unsupported diff;
4. explicit refresh when the native generation is stale.

No raw native path, Git argv, session id, executor metadata, or debug record is
rendered. Repository ids and file paths are portable project-relative strings.

## Existing foundation

The implementation must compose, not replace, these accepted pieces:

- `keeper-git-executor`: bounded discovery, porcelain v2 status, fixed-template
  diff, nested-repository containment and unsafe-config denial;
- `GitGateway` and `TauriGitGateway`: strict renderer-side DTO validation;
- `GitChangesView`: the existing read-only visual primitive;
- `ResourcePane`, `Workbench`, and `App`: the established contextual state
  flow, project-switch isolation and generation guards.

## Authority and state model

### Native authority

The Tauri host owns a `GitCommandState` managed alongside `AppState`. It holds
an ephemeral per-project authority snapshot:

- the current generation;
- the discovered repository ids and availability flags;
- the fixed, host-resolved Git executable through `GitReadExecutor`.

`discover_git_repositories(project_id)` resolves the registered project through
`AppState`, performs bounded discovery, publishes only portable ids, and
atomically replaces that project's authority snapshot with a fresh generation.

`get_git_status` and `open_git_diff` accept only `projectId`, `repositoryId`,
`generation`, and for diff a portable `path`. They reject calls when:

- the project is no longer registered;
- the generation does not equal the latest discovery for that project;
- the repository id was not in that discovery or was unavailable;
- containment, configuration, identity, output, or path checks fail below the
  command layer.

The renderer never supplies a native root or executable. The host never offers
arbitrary argv. Git executable resolution is host-only, canonicalized and
covered by tests; a missing executable maps to `git_unavailable`.

### Renderer state

`App` owns the `GitGateway` and project-scoped Git refresh lifecycle.
`Workbench` owns only the currently displayed Changes mode and delegates the
pane body to `ResourcePane`/`GitChangesView`.

Every async result is guarded by the selected project id plus the active Git
generation. A project switch immediately clears discovery, status, selected
path, diff, loading and error state before the next request. A late result from
the previous project or generation is discarded. Restart recovery begins with
no native Git authority and therefore performs a fresh discovery; no ad hoc
JSON cache is introduced.

## DTO convergence

`apps/desktop/src/lib/gitGateway.ts` is the single wire-contract source.
Duplicate Git wire DTOs in `executionTypes.ts` must be removed or converted to
view-only state that imports the gateway types.

`GitChangesView` receives the authoritative `GitStatusView` and `GitDiffView`
from `gitGateway.ts`. UI-only conditions such as loading, a path-free public
error and stale refresh guidance are separate props; they are never added to
or confused with native DTO fields. Binary status is inferred only from an
opened binary diff, not invented on status entries.

## Error vocabulary

Only the existing public codes cross Tauri:

- `git_project_not_found`
- `git_path_rejected`
- `git_repository_not_found`
- `git_unavailable`
- `git_generation_stale`
- `git_generation_mismatch`

Messages are path-free. The renderer normalizes malformed or unknown responses
to `git_unavailable`. Stale/mismatch errors keep the pane open and offer a
refresh; they do not merge old status with new discovery.

## Interaction contract

- Opening Changes triggers discovery, chooses the project-root repository when
  available, otherwise the first available repository, and fetches status.
- Nested repositories use a small selector only when more than one repository
  is discovered.
- Selecting a file requests exactly one diff for the current repository and
  generation.
- Refresh performs a new discovery and invalidates the prior status/diff.
- Empty, unavailable, detached HEAD, rename, conflict, untracked, binary,
  oversized/unsupported and truncated states are explicit and compact.
- No action in the pane changes the working tree, index, HEAD or configuration.

## Acceptance boundary

Source/unit acceptance proves strict DTOs, native command authority, project
and generation isolation, and the contextual UI state flow. Real Windows
desktop acceptance proves discovery/status/diff through Tauri against temporary
Git fixtures including ordinary, untracked, rename, binary, detached HEAD,
nested repository, unsafe configuration denial and project switching.

D3B4 is accepted only after the controller personally reviews every changed
file and runs the focused gates, `pnpm check`, production build, full serial
desktop E2E, formatting/diff checks, and a residual-process check.

## Explicitly out of scope

- stage/unstage, commit, amend, checkout, branch, merge, rebase or reset;
- discard/delete/restore working-tree content;
- terminal or arbitrary Git commands;
- diff editing or patch application;
- remote operations, credentials, signing and key management;
- Obsidian or later-stage project-operations features.

