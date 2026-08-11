# Git Changes UI (stage / commit / push) — Execution Report

Date: 2026-08-11

Implementation status: COMPLETE

Worker: grok1 (renderer only). Controller: Claude.

Scope: Changes panel wiring for structured Git mutations (stage, unstage,
commit, push) in the desktop renderer. No host, crate, script, or Pillow
repository Git writes.

No command output was piped through `tail` or another exit-code-masking
command.

## 1. Exit-code table

| Gate | Exit code | Result | Evidence |
| --- | ---: | --- | --- |
| `corepack pnpm typecheck` | 0 | PASS | Workspace recursive typecheck; `@pillow/desktop` `tsc --noEmit` clean. |
| Scoped Git-related Vitest (see below) | 0 | PASS | 119/119 passed across five files. |
| `corepack pnpm --filter @pillow/desktop test` (full suite, run 1) | 1 | FLAKY / OUT-OF-SCOPE | 483 passed, 5 failed: timeouts / input races in `conversation-fix-pass2`, `conversation-list-race`, `login-guidance`, `runtime-slots-ui` — none in Git Changes files. |
| Same four out-of-scope files re-run alone | 0 | PASS | 28/28 passed; proves full-suite failures were parallel-load flakes, not this task. |
| Full suite re-run | 1 | FLAKY / OUT-OF-SCOPE | 486 passed, 2 failed: same pattern (`conversation-fix-pass2` timeout, `runtime-slots-ui` timeout). Git suites green inside the same run (18 + 40 + 28 + 19). |
| cargo / desktop E2E / worker gate | N/A | NOT RUN | Explicitly prohibited for this task. |

### Scoped Git-related command (exit 0)

```text
corepack pnpm --filter @pillow/desktop exec vitest run \
  tests/c4-git-changes-integration.test.tsx \
  tests/c4-execution-primitives.test.tsx \
  tests/tauri-git-gateway.test.ts \
  tests/resource-pane.test.tsx \
  tests/c4-gateway-contract.test.ts
```

Result: Test Files 5 passed; Tests 119 passed.

## 2. Section 5 requirements — tests and assertions

| Requirement | Test location | What is asserted (not “rendered only”) |
| --- | --- | --- |
| 1. Stage / unstage call gateway with correct args | `c4-git-changes-integration.test.tsx` — *stages and unstages through the gateway…* | After click, `stageCalls` / `unstageCalls` equal exact `{ projectId, repositoryId, generation, paths: ["src/app.ts"] }`; status is re-fetched (`statusCalls` increases). |
| 1b. View-level path args | `c4-execution-primitives.test.tsx` — *stages and unstages via callbacks…* | `onStagePath` / `onUnstagePath` called with portable path; untracked shows stage only (no unstage control). |
| 2. Empty commit blocked; commit refreshes list | Integration — *rejects empty commit messages…*; primitives — *blocks empty commit messages…* | Submit disabled / no `commitCalls` when empty; non-empty message reaches gateway; after commit mock returns clean status, `git-changes-clean` appears. |
| 3. Push requires confirm; open alone inert | Integration — *does not call push gateway until the confirm step…*; primitives — *requires push confirmation…* | `pushCalls === []` after “准备推送…”; only after confirm submit does gateway receive `{ confirmed: true, remoteName, branchName }`. |
| 4. Host error text never in DOM | Integration — *maps mutation failures…*; primitives — *surfaces fixed path-free mutation errors…*; `tauri-git-gateway.test.ts` mutation host-code mapping | Thrown host leak (`user:sekrit@…`, `non-fast-forward`, native path) absent from pane text; UI shows only fixed `GIT_ERROR_MESSAGES.*`. |
| 5. Changes + resource browser coexist | Integration — *keeps Changes mode optional…* | Default body loads resource browser (`listProjectResources` + `ctx-resource-pane`); Git discovery starts only after explicit Changes tab; switching back restores resources and unmounts Changes; mode switcher remains. |
| Gateway invoke contract | `tauri-git-gateway.test.ts` — stage/unstage/commit/push invokes | Exact command names and camelCase payloads; `confirmed: false` never invokes host; mutation DTOs closed-key normalized. |
| No discard / hard reset entry | primitives — *disables mutation controls while mutating…* | DOM text has no discard / `reset --hard` / 硬重置 / 丢弃. |
| In-flight disable | same + `data-mutating` | Stage / commit / push controls disabled while `mutating`; double-submit blocked at App (`mutating` gate). |

## 3. Push confirmation — exact copy

Exported constants on `GitChangesView` (pinned by tests):

| Role | Exact string |
| --- | --- |
| Primary open control (not a push) | `准备推送…` |
| Confirm panel title | `推送是对外发布` |
| Confirm body | `即将把本地分支推送到远端。此操作会发布到远程仓库，难以撤回。请确认远端名称与分支后再继续。` |
| Confirm submit | `确认推送到远端` |
| Cancel | `取消` |

Flow:

1. User clicks **准备推送…** → confirmation group opens; **no** gateway call.
2. UI shows editable **远端** (default `origin`) and read-only **分支** from current status branch (disabled when detached / no branch).
3. Only **确认推送到远端** calls `onPush({ remoteName, branchName })`, which App maps to `pushGitBranch` with `confirmation.confirmed: true`.

## 4. Host-side missing fields (for controller)

Do **not** invent these in the renderer; host should admit them in a later task if product needs them.

| Gap | Why it matters | Current renderer workaround |
| --- | --- | --- |
| Status entry staged / worktree split (e.g. `xy`, `staged: boolean`, or index vs worktree kinds) | Executor already has porcelain `xy`; wire DTO only exposes `kind` + path. Cannot reliably show separate staged vs unstaged lists. | Heuristic: untracked → stage only; other non-ignored → stage + unstage. |
| Distinct push failure codes for auth vs remote reject vs non-fast-forward | Host maps most process failures to opaque `git_mutation_failed`. UI cannot branch copy by root cause without echoing host stderr. | Single fixed actionable message covering credentials, remote access, and fast-forward; never echoes host text / remote URLs. |
| Remotes discovery / default upstream | Push needs `remoteName`; no list-remotes or tracking-branch DTO. | Default remote draft `origin`; user can edit in the confirm panel before confirm. |
| Generation bump after mutation | Receipt reuses discovery generation; status refresh reuses same expect. | Fine today; if host later invalidates generation on mutation, renderer already treats stale/mismatch codes. |

Mutation public error codes **were** already on the host (`git_message_rejected`, `git_branch_rejected`, `git_remote_rejected`, `git_push_confirmation_required`, `git_mutation_failed`) and are now admitted in the renderer closed list.

## 5. Write-boundary audit

### Written by this worker

- `apps/desktop/src/lib/gitGateway.ts` — mutation types, gateway methods, public error codes + fixed messages
- `apps/desktop/src/lib/tauriGitGateway.ts` — closed-key mutation normalizer + four invokes
- `apps/desktop/src/features/execution/GitChangesView.tsx` — stage / unstage / commit / push UI
- `apps/desktop/src/features/workbench/ResourcePane.tsx` — `GitChangesPaneModel` mutation fields + pass-through
- `apps/desktop/src/App.tsx` — mutation handlers, `mutating` state, status refresh after success
- `apps/desktop/src/styles/execution.css` — mutation control styles
- `apps/desktop/tests/c4-execution-primitives.test.tsx`
- `apps/desktop/tests/c4-git-changes-integration.test.tsx`
- `apps/desktop/tests/tauri-git-gateway.test.ts`
- `apps/desktop/tests/resource-pane.test.tsx` (model fixture + entry select testid)
- this report

### Explicitly not written

- `crates/**`, `apps/desktop/src-tauri/**`
- `scripts/**` (other worker)
- lockfiles / dependencies
- any Git stage / commit / push / reset against the Pillow working tree

### Residual risks

- Full desktop Vitest under default parallel load still flakes on unrelated conversation / runtime-slots timeouts (exit 1). Scoped Git suites and isolated re-runs of those files are green. Treat full-suite exit 1 as env flake unless a Git-file assertion fails.
- Staged/unstaged UX is heuristic until host exposes index/worktree fields; unstage on a purely worktree-modified path may surface `git_mutation_failed`.
- Push remote defaults to `origin` without discovery; wrong remote name fails closed via host validation + fixed UI copy.
- Auth for HTTPS push remains a host/credential concern (mutation backend clean-env); UI only surfaces the opaque failure message.
- Desktop E2E and cargo gates were not run per task rules.

## 6. Architecture notes (renderer)

- Resource pane still never calls the Git gateway; App owns gateway + scope generation.
- Mutations set `mutating` and disable controls; success path reloads status for the current expect tuple.
- Host errors are always normalized through `normalizeGitError` / fixed `GIT_ERROR_MESSAGES`; raw reject payloads never enter React state as display text.
- No discard / reset-hard control is exposed.

---

`GIT-CHANGES-UI-COMPLETE`
