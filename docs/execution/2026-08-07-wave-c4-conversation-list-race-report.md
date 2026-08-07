# C4 — `conversations_loaded` 乱序竞态修复报告

**Worker:** grok-worker  
**Controller session:** `<session-id>`  
**Date:** 2026-08-07  
**Worktree:** `<repo>`  
**Scope:** Fix structural race where a late project-effect `listConversations` snapshot replaces a fresher send-path list, dropping `selectedConversationId` from ownership and unmounting `composer-cancel` (D3B3 cancel-tree flake root cause).

---

## 1. Changed files

| Path | Change |
| --- | --- |
| `apps/desktop/src/App.tsx` | Per-project monotonic list-seq counter; stamp `listSeq` on both list paths before dispatch |
| `apps/desktop/src/features/conversations/conversationReducer.ts` | `conversationListSeqByProject` state; discard `conversations_loaded` when `listSeq` is not strictly fresher; export `shouldApplyConversationListSeq` |
| `apps/desktop/tests/conversation-list-race.test.tsx` | **New** — four required race regressions (+ one ownership helper case) |
| `docs/execution/2026-08-07-wave-c4-conversation-list-race-report.md` | This report |

**Not modified (whitelist residual):**  
`conversation-workbench.test.tsx`, `conversation-fix-pass2.test.tsx` — existing suite still green without edits.  
`ResourcePane.tsx` / `Workbench.tsx` / wire DTOs / gateway / Tauri — untouched.

**Note on types:** `ConversationAction` / `ConversationUiState` for the list-seq contract are extended in `conversationReducer.ts` (intersection + discriminated override) so `conversationTypes.ts` was not required. Base type module remains unchanged. Runtime-event / timeline-merge paths cast through `as ConversationUiState` because those helpers still type against the base state but spread `...state` and preserve the seq map at runtime.

---

## 2. Freshness sequence implementation

### Allocation (request start)

`App.tsx` holds `conversationListSeqCounterRef: Record<string, number>`.

```ts
function nextConversationListSeq(projectId: string): number {
  const next = (conversationListSeqCounterRef.current[projectId] ?? 0) + 1;
  conversationListSeqCounterRef.current[projectId] = next;
  return next;
}
```

- **Project-select effect** (`listConversations` on `activeProject`): allocate `listSeq` *before* the promise; on fulfill, if `active` still true, dispatch with that `listSeq`.
- **Accepted-send path** (post-`sendTurn` refresh): allocate `listSeq` *before* `Promise.allSettled([loadTimeline, listConversations])`; on fulfill, if `isScopeCurrent(...)`, dispatch with that `listSeq`.

### Apply (reducer)

State field: `conversationListSeqByProject: Record<string, number>`.

On `conversations_loaded`:

1. Read `applied = state.conversationListSeqByProject[projectId]`.
2. If `!(incoming listSeq > (applied ?? 0))` → return `state` unchanged (no replace).
3. Else replace `conversationsByProject[projectId]` and store the new applied seq.

Helper: `shouldApplyConversationListSeq(applied, incoming)` — strict greater-than.

---

## 3. Design rules checklist (§2)

| # | Rule | How satisfied |
| --- | --- | --- |
| 1 | Monotonic seq per project at each `listConversations` start | `nextConversationListSeq` on both paths |
| 2 | Strictly older results discarded | Reducer compares before replace |
| 3 | Both call paths protected | Effect + send refresh both stamp + dispatch `listSeq` |
| 4 | Project-scope isolation not weakened | `active` cleanup and `isScopeCurrent(projectId, generation)` unchanged; late wrong-project results still never dispatch |
| 5 | Project switch clear scope unchanged | `project_cleared` / `clearConversationScope` behavior unchanged; seq map is per-project and not cleared (monotonic forever is fine) |
| 6 | No new Tauri / wire DTO / gateway / authority changes | Only renderer App + reducer + tests |
| 7 | Primary fix is freshness, not “re-insert selected” patch | No selected-conversation re-merge in reducer; optional selected protection **not** added |

---

## 4. Regression tests (§4)

File: `apps/desktop/tests/conversation-list-race.test.tsx`

| # | Test name | Assertion points |
| --- | --- | --- |
| 1 | `conversation list freshness (reducer) > discards an older listSeq so a later-arriving stale snapshot cannot overwrite` | After seq=2 with `[old,new]`, seq=1 with `[old]` is a no-op (`toBe(state)`); helper rejects equal/older |
| 2 | `… > keeps the newly created conversation selected after a late older list lands` | Deferred lists: resolve send-path list (with new conv) first, then empty project-effect list; row still shows title + `aria-current="page"` |
| 3 | `… > keeps composer-cancel mounted when a late older list arrives mid-run` | Same ordering with running timeline; `composer-cancel` visible before and after late empty list |
| 4 | `conversation list project isolation … > drops a late list result for the previous project after switch` | Select A (hold list) → switch B → resolve B → late resolve A; “A only” never appears under B |

Extra (ownership): `listSeq helper and ownership after stale discard > preserves selected ownership when stale list is rejected` — pure reducer proof that `conversationBelongsToProject` + `isRunActive(selectActiveRun)` survive a rejected empty list.

---

## 5. Gate results

Commands run (only these; no desktop E2E):

```text
corepack pnpm --filter @pillow/desktop typecheck
→ exit 0

corepack pnpm --filter @pillow/desktop exec vitest run \
  tests/conversation-list-race.test.tsx \
  tests/conversation-workbench.test.tsx \
  tests/conversation-fix-pass2.test.tsx \
  tests/agent-activity-timeline-integration.test.tsx \
  tests/write-approval-workbench.test.tsx \
  --no-cache
→ exit 0
→ 5 files, 75 tests passed

git diff --check
→ exit 0
(LF/CRLF “will be replaced” warnings only; no whitespace errors)

git diff --cached --name-only
→ empty (exit 0)
```

---

## 6. Staged area

**Empty.** No `git add` / commit / stash / reset performed. Working tree remains dirty as found.

---

## 7. Residual risks

1. **Window between `conversation_created` and first applied `conversations_loaded`:** If a *never-applied* older list (seq=1, empty) lands *after* `conversation_created` but *before* any fresher list is applied, it can still replace the list (seq=1 > applied 0). The cancel-tree flake case is the stronger one: fresher list already applied, then older arrives — now fixed. Closing the pre-first-apply window would need either not applying lists that drop the selected id (patch-style) or advancing the floor on create; not done per §2.7 primary-fix guidance.
2. **Counter vs applied state:** Allocation lives in an App ref; applied floor lives in reducer state. A full App remount resets both; mid-session they stay consistent because every dispatch carries the allocated seq.
3. **Desktop E2E not run here** (controller-owned serial suite). Unit/integration tests model the race with deferred `listConversations` promises; controller re-runs full serial desktop E2E for final checkpoint.

---

## 8. Explicitly not done

- No desktop E2E (`test:e2e`, wave acceptance scripts)
- No git commit / add / stash / reset / clean / checkout / restore
- No changes to `ResourcePane.tsx`, `Workbench.tsx`, Tauri commands, gateway wire, or authority/privacy
- No “re-insert selected conversation into stale list” patch as primary fix
- No edits to `.superpowers/` evidence
- No process kill / auth file reads
- Did not re-argue the defect; followed controller diagnosis

---

## 9. Completion

Ready for controller source review + exclusive serial desktop E2E re-verify of cancel-tree.
