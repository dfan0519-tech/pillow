# C2-E Isolation, Restart, and Desktop Acceptance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `subagent-driven-development` or an equivalent narrow worker workflow.
> Steps use checkbox (`- [ ]`) syntax for controller tracking.

**Goal:** Prove C2 through a real Tauri/WebView2 lifecycle with two temporary
projects, project-scoped resources and activity, durable recents, restart
recovery, keyboard reachability, normal window close, and zero residual task
processes.

**Final status (2026-08-01):** Accepted. Focused C2-E passed 1/1 and the final
complete desktop suite passed 7/7. Production code remained unchanged by the
C2-E harness work.

**Architecture:** Keep production code unchanged during the first pass. A Node
orchestrator launches the real Pillow desktop with temporary app data and a
temporary WebView2 profile, attaches to the real WebView2 through a
test-scoped CDP port, and drives the public UI. Two focused helper modules own
resource/preview and conversation/activity assertions. The outer Node test
checks only the stable machine-readable completion record.

**Tech Stack:** Node test runner, Playwright Core over WebView2 CDP, Tauri 2,
React 19, SQLite, existing `scripts/lib/desktop-process.mjs` supervision.

## Global Constraints

- Work only in `<workspace>\pillow`; never edit `<workspace>\VaultSystem\keeper`.
- Preserve the accepted dirty tree; no reset, overwrite, stage, or commit.
- C2 stays read-only: no product file writes, terminal, Git, Obsidian, keys,
  MCP, network tools, or runtime expansion.
- Test fixture creation may write only inside a unique `mkdtemp` root owned by
  the acceptance process.
- Project A/B paths and markers must be unique and must never be displayed in
  the opposite project.
- Use the existing real Tauri IPC, project repository, conversation service,
  resource gateway, SQLite database, and fake runtime fixture.
- Close each desktop phase through a normal window-close request; forced
  termination is cleanup fallback only and must fail the acceptance result.
- Record exact child PIDs and remove only the acceptance-owned temporary root
  after every recorded PID is dead.
- Worker file whitelists below are exclusive and do not overlap.
- Workers report changed files, focused tests, residual risks, and a Keeper
  completion message. Worker completion is not controller acceptance.

## Frozen Scenario Contract

The orchestrator prints exactly one terminal record:

```text
KEEPER_WAVE_C2_RESOURCE_ACCEPTANCE {"schemaVersion":1,"status":"passed",...}
```

The JSON record must contain:

```js
{
  schemaVersion: 1,
  status: "passed",
  ipcBoundary: "real-tauri-webview",
  projectIsolation: "passed",
  restartRecovery: "passed",
  resourceCoverage: "tree-search-preview-recents",
  activityCoverage: "conversation-owned-real-events",
  keyboardReachable: "passed",
  normalClose: "passed",
  processCleanup: "passed",
  phases: ["register-a", "register-b", "create", "restore"]
}
```

The two projects contain distinct portable resources:

```text
Project A: notes/a-only.md -> PILLOW_C2_A_ONLY_MARKER
Project B: notes/b-only.md -> PILLOW_C2_B_ONLY_MARKER
```

The create phase must:

1. select A through the public sidebar;
2. open the contextual resource pane by keyboard;
3. prove A tree, search, preview, and recents contain A data and never B data;
4. create an A conversation through the public UI and send
   `C2 A activity persistence`;
5. prove the real fake-runtime activity disclosure shows plan, tool, sub-agent,
   output/artifact summaries from persisted events;
6. select B and prove B resources never contain A data and A conversation
   activity is absent;
7. exercise B preview so B has its own recent-resource row;
8. close through the normal window-close path.

The restore phase reuses the same app data and WebView2 profile, then proves:

1. A recents contain only `notes/a-only.md`;
2. A conversation, prompt, assistant output, and Agent activity restore;
3. B recents contain only `notes/b-only.md`;
4. B still cannot display A prompt, A activity, A path, or A marker;
5. the desktop closes normally and leaves no owned process or listener.

## Shared Helper Interfaces

`scripts/lib/c2-resource-ui-acceptance.mjs` exports:

```js
export async function selectProject(page, projectName);
export async function exerciseProjectResources({
  page,
  ownPath,
  ownMarker,
  foreignPath,
  foreignMarker,
});
export async function assertRestoredProjectRecents({
  page,
  ownPath,
  ownMarker,
  foreignPath,
  foreignMarker,
});
```

`scripts/lib/c2-activity-ui-acceptance.mjs` exports:

```js
export async function createConversationActivity({ page, prompt });
export async function assertConversationActivityRestored({
  page,
  prompt,
  foreignPrompt,
});
export async function assertConversationActivityAbsent({
  page,
  forbiddenPrompt,
});
```

Helpers return JSON-serializable evidence only. They do not launch processes,
write fixture files, select a different project, close the app, or delete
paths.

---

### Task 1: Real Tauri lifecycle orchestrator and outer E2E

**Owner:** grok1

**Files:**

- Create: `scripts/wave-c2-resource-acceptance.mjs`
- Create: `apps/desktop/tests/e2e/c2-resource-workbench.spec.ts`
- Modify: `apps/desktop/package.json`
- Create:
  `docs/execution/2026-07-31-wave-c2e-orchestrator-report.md`

- [x] Create A/B fixture roots and files beneath one unique temporary root.
- [x] Register A and B through the existing native acceptance path against one
      temporary app-data directory.
- [x] Launch create/restore desktop phases with `PILLOW_RUNTIME=fake`, a unique
      CDP port, and the acceptance-owned WebView2 profile.
- [x] Import and sequence both frozen helper interfaces.
- [x] Send a normal Windows close request and require graceful process exit.
- [x] Verify PIDs/listeners before deleting only the owned temporary root.
- [x] Emit the frozen terminal record on success and a redacted failed record
      on error.
- [x] Add the new E2E to `test:e2e`; the wrapper asserts every frozen field.

### Task 2: Conversation-owned activity create/restore helper

**Owner:** grok2

**Files:**

- Create: `scripts/lib/c2-activity-ui-acceptance.mjs`
- Create:
  `docs/execution/2026-07-31-wave-c2e-activity-driver-report.md`

- [x] Drive only public accessible UI selectors.
- [x] Create a conversation, send the supplied prompt, and wait for terminal
      assistant output.
- [x] Expand `agent-activity-disclosure` progressively.
- [x] Prove the rendered activity comes from the committed fake runtime:
      `Scan files`, `Write summary`, `list_dir`, `explore`, output/artifact
      disclosure, and completed status.
- [x] On restore, prove prompt, assistant content, and activity return without
      resending the turn.
- [x] Prove the forbidden project prompt/activity is absent.

### Task 3: Project resource isolation and recents helper

**Owner:** grok3

**Files:**

- Create: `scripts/lib/c2-resource-ui-acceptance.mjs`
- Create:
  `docs/execution/2026-07-31-wave-c2e-resource-driver-report.md`

- [x] Select projects through sidebar accessible names and wait for the winning
      project heading.
- [x] Open the resource panel with keyboard focus and activation.
- [x] Prove tree and search return only the own portable path/marker.
- [x] Open bounded Markdown preview and prove the foreign marker/path are
      absent from the resource context.
- [x] Clear search before recents assertions.
- [x] On restore, prove the own recent path opens the own preview while the
      foreign recent path/marker is absent.

### Task 4: Integration repair wave

**Owner:** controller dispatches one Grok fixer only if the assembled focused
E2E is red.

- [x] The controller first reproduces and classifies each failure.
- [x] Any fix gets a new non-overlapping whitelist and RED/GREEN evidence.
- [x] No production edit is allowed merely to simplify automation.

### Task 5: Independent review and controller acceptance

**Owner:** Codex2 review; Codex Desktop controller acceptance.

- [x] After focused E2E is green, Codex2 reviews spec compliance and code
      quality, with Critical/Important findings blocking acceptance.
- [x] The controller personally inspects all changed files and runs focused
      E2E, desktop Vitest/typecheck/build, `git diff --check`, staging, HEAD,
      and residual-process gates.
- [x] The controller alone writes
      `docs/verification/2026-08-01-wave-c2-acceptance.md`.
- [x] Task 5 of the parent C2 plan begins only after C2-E acceptance.

## Plan Self-Review

- Spec coverage: two-project isolation, tree/search/preview/recents,
  conversation-owned activity, restart recovery, keyboard reachability, normal
  close, and cleanup all have named assertions.
- Scope: the first implementation pass changes acceptance-only scripts/tests
  and package wiring; production changes require a controller-issued repair
  brief.
- Type consistency: the three helper exports match the orchestrator imports.
- Authority: workers cannot accept C2-E, stage, commit, or write the final
  acceptance document.
