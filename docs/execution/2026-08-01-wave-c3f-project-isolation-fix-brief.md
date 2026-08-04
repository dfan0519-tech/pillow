# Pillow C3-F project-switch approval isolation fix

## Status and authority

- Controller session: `<keeper-session>`
- Before reading or editing, boot through
  `POST http://127.0.0.1:7890/api/agent-bridge/boot` with project `pillow`,
  task `c3f-project-isolation-fix`, agent name `grok1`, role `worker`, and
  parent session `<keeper-session>`.
- HEAD must remain `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`.
- The existing dirty C1/C2/C3 working tree is accepted input. Do not reset,
  restore, stage, commit, package, publish, or modify old Keeper code.
- This is a narrow renderer product fix. The worker reports completion; the
  Codex Desktop controller independently reviews and accepts or rejects it.

## Proven live blocker

In the real Tauri/WebView2 C3-F acceptance, Project A owns a live pending
write proposal. After selecting Project B, the sidebar and active heading both
show B, but `write-approval-bar` from A remains mounted after an 8 second
settle. The preceding six governed-write scenarios are green.

Controller inspection found the unsafe boundary: `App` derives
`activeConversation` from the active project's conversation list, but passes
the raw global `selectedConversationId`, `selectActiveTimeline`, and
`selectActiveRun` into `Workbench`. A stale/late A selection can therefore
keep A's approval surface visible while B is active. Workbench's effect-based
cleanup and async generation guard are necessary but are not a render-time
ownership proof.

## Required contract

1. `Workbench` may receive/render a conversation, timeline, run, proposal,
   grant-backed action, diff, or write outcome only when the selected
   conversation is proven to belong to the currently active project.
2. Project A pending -> select Project B: A's approval bar and every A write
   surface are absent at the B render boundary; do not rely on an effect,
   timeout, or polling to clear them later.
3. Switching back to A may restore the exact pending proposal only by
   re-listing native authority for A's owning conversation. Never copy or
   expose an approval grant in React state, DOM, log, snapshot, or error.
4. A stale project/conversation list or timeline response must not rehydrate A
   under B. Existing last-request-wins and project-generation semantics remain.
5. Do not cancel the A write merely because the user views B. Background run
   continuity remains conversation-owned.
6. No interval polling, debug metadata, native root path, raw session/run/tool
   identifiers, or additional dashboards.

## Write whitelist

- `apps/desktop/src/App.tsx`
- `apps/desktop/src/features/workbench/Workbench.tsx` only if a second
  render-boundary defense is demonstrably required
- `apps/desktop/tests/write-approval-workbench.test.tsx`
- `apps/desktop/tests/project-workbench.test.tsx`
- `docs/execution/2026-08-01-wave-c3f-project-isolation-fix-report.md`

Do not edit native, Rust, storage, protocol, runtime, gateway, acceptance
harness, package, roadmap, checkpoint, or unrelated renderer files.

## TDD and focused verification

First add a regression that fails on the current tree and proves:

- A is selected with a pending proposal and bar visible;
- selecting B makes the active heading B and removes A's bar/write evidence;
- a late A conversation/timeline/list result cannot restore A under B;
- switching back to A and selecting its conversation re-lists and restores the
  same pending proposal from the gateway;
- the grant and both projects' native root paths are absent from body text and
  HTML attributes.

Run and report exact counts/output for:

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/write-approval-workbench.test.tsx tests/project-workbench.test.tsx
pnpm --filter @pillow/desktop typecheck
pnpm --filter @pillow/desktop build
git diff --check -- apps/desktop/src/App.tsx apps/desktop/src/features/workbench/Workbench.tsx apps/desktop/tests/write-approval-workbench.test.tsx apps/desktop/tests/project-workbench.test.tsx docs/execution/2026-08-01-wave-c3f-project-isolation-fix-report.md
git diff --cached --name-only
git rev-parse HEAD
```

## Completion report

Write `docs/execution/2026-08-01-wave-c3f-project-isolation-fix-report.md`
with: root cause, RED evidence, changed files, contract proof, exact tests,
residual risks, HEAD/staging, residual worker processes, and one final line
`COMPLETION: DONE` or `COMPLETION: BLOCKED`.

Before exit, POST a concise completion message to Keeper Bridge for the worker
session. Include completion status, changed files, tests, residual risks, and
state explicitly that worker completion is not controller acceptance.
