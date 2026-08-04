# Wave C3-F project-switch approval isolation fix — worker report

Controller session: `<keeper-session>`
Worker session: `<keeper-session>`
Worker: `grok1`
Task: `c3f-project-isolation-fix`
Date: 2026-08-01

**Worker completion is not controller acceptance.**

## Root cause

`App` already derived `activeConversation` (title) from the active project's conversation list, but passed the **raw global** `conversationState.selectedConversationId`, `selectActiveTimeline(conversationState)`, and `selectActiveRun(conversationState)` into `Workbench`.

When a late or raced conversation selection left `selectedConversationId` pointing at Project A's conversation while Project B was the active project, Workbench still received `conversationId = A` and re-listed A's pending proposal. The existing Workbench effect-based `clearWriteScope` / generation guards are necessary but are not a **render-time ownership proof**, so A's `write-approval-bar` could remain mounted under B's heading after multi-second settle (live C3-F two-projects blocker).

## RED evidence

Regression added first (before product fix):

`write approval workbench integration > project switch drops A write surfaces at B render boundary without effect settle`

Reproduction path:

1. Select Project A → select conversation A → pending bar visible (`src/alpha-only.ts`).
2. Click Project B with deferred `selectProject` (activation in flight).
3. Re-select conversation A while A is still the painted active project (conversation rows are not `busy`-disabled).
4. Resolve B activation.

**Observed before fix:**

- `active-project-name` text: Project B
- `write-approval-bar` still mounted with A's path `src/alpha-only.ts`
- Assertion `expect(screen.queryByTestId("write-approval-bar")).toBeNull()` failed

Vitest excerpt (RED):

```text
FAIL  tests/write-approval-workbench.test.tsx > ... project switch drops A write surfaces...
AssertionError: expected <div ... data-testid="write-approval-bar" ...> to be null
```

## Changed files (whitelist only)

| Path | Change |
| --- | --- |
| `apps/desktop/src/App.tsx` | Render-time ownership proof for Workbench/sidebar props; owned-only write refresh bump; `selectConversation` rejects foreign selection and drops late timeline under foreign project |
| `apps/desktop/tests/write-approval-workbench.test.tsx` | App-level isolation regression: A pending → B bar/path gone at render boundary; late A timeline/list cannot rehydrate; switch back re-lists same pending; grant + both native roots absent from text/HTML |
| `apps/desktop/tests/project-workbench.test.tsx` | Render-boundary ownership: foreign conversation timeline cannot paint under active B; native roots absent |
| `docs/execution/2026-08-01-wave-c3f-project-isolation-fix-report.md` | This report |

`Workbench.tsx` was **not** edited for this fix (App render boundary is sufficient; Workbench's conversation-id match gate remains a second line of defense already present).

No native, Rust, storage, protocol, runtime, gateway, harness, package, roadmap, or unrelated renderer edits.

## Contract proof

1. **Ownership proof at render boundary**
   `selectionOwnedByActiveProject = conversationBelongsToProject(state, selectedConversationId, activeProject.id)`.
   Workbench receives only:
   - `conversationId = ownedConversationId`
   - `timeline` / `run` only when ownership holds
   - `conversationTitle` from owned conversation
   Sidebar `selectedConversationId` also uses `ownedConversationId`.

2. **A pending → select B**
   Foreign selection yields `ownedConversationId = null` immediately when B is active, so A's bar, path, summary, and conversation title cannot mount under B without waiting for an effect, timeout, or poll.

3. **Switch back to A**
   Selecting A's conversation re-lists via Workbench's existing scope entry path (`listWriteProposals(conversationId)`). Grant stays in Workbench ref map only after gateway list; never copied into React state/DOM/log in this change.

4. **Stale A results under B**
   Late timeline commits after project leave are dropped in `selectConversation`. Late list results under wrong scope remain suppressed by existing Workbench generation guard. Render ownership prevents painting even if global `selectedConversationId` is still A.

5. **Background run continuity**
   No cancel of A's write on view of B. Global event subscription continues; write refresh token only bumps for events whose conversation is selected **and** owned by the active project.

6. **Privacy**
   No interval polling, debug metadata, native root path, raw session/run/tool identifiers, or dashboards added. Tests assert grant + both project root paths absent from `body` text and HTML attributes.

## Exact tests / gates

```text
pnpm --filter @pillow/desktop exec vitest run tests/write-approval-workbench.test.tsx tests/project-workbench.test.tsx
```

```text
Test Files  2 passed (2)
     Tests  32 passed (32)
```

```text
pnpm --filter @pillow/desktop typecheck
→ tsc --noEmit exit 0

pnpm --filter @pillow/desktop build
→ vite build exit 0 (73 modules; dist produced)

git diff --check -- apps/desktop/src/App.tsx apps/desktop/src/features/workbench/Workbench.tsx apps/desktop/tests/write-approval-workbench.test.tsx apps/desktop/tests/project-workbench.test.tsx docs/execution/2026-08-01-wave-c3f-project-isolation-fix-report.md
→ exit 0 (LF/CRLF warning only on pre-existing Workbench.tsx; not edited this pass)

git diff --cached --name-only
→ (empty)

git rev-parse HEAD
→ 7b7f70f7360131cd1cfd984a8b19bdfb5725c21b
```

## Residual risks

- Live Tauri/WebView2 C3-F two-projects scenario is not re-run in this worker pass; controller should re-run the governed-write acceptance harness for final acceptance.
- Global `selectedConversationId` may still hold a foreign id until the next `clearConversationScope` / re-select; it is no longer authority for Workbench paint, but residual global state remains until cleared by existing activation paths.
- Conversation rows remain clickable while `busy` during project activation; ownership gate closes the paint leak; optionally disabling conversation clicks while busy would reduce race frequency but is out of this whitelist scope.

## HEAD / staging / residual processes

| Item | Value |
| --- | --- |
| HEAD | `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b` (unchanged) |
| Staging | empty (no stage/commit) |
| Residual targeted vitest/tsc/pnpm worker processes | 0 |

## Completion

Worker product fix complete in the dirty tree. No stage, commit, package, or publish.

**Worker completion is not controller acceptance.**

COMPLETION: DONE
