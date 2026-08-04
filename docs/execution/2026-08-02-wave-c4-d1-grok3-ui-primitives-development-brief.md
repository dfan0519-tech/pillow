# C4-D1 Grok3 development brief — isolated execution UI primitives

Role: real Grok CLI slot `grok3`, implementation worker.

Read the three AGENTS files, the accepted C4 design, the two local interaction
research reports and Task 4 of the C4 implementation plan. Boot Keeper as
worker for task `c4-d1-execution-ui-primitives` before editing.

## Deliverable

Build controlled, unmounted C4 components with TDD: command activity,
approve-once bar, bounded terminal-output view and Git Changes view. The
components receive sanitized DTOs and callbacks only. They must not import
Tauri, fetch, persist, infer terminal truth, modify selected project or expose
native paths, grants or raw identifiers. Use origin `agent|user` and the exact
command-state union from Task 4. Preserve conversation-centred progressive
disclosure and keyboard behavior.

## Write allowlist

- `apps/desktop/src/features/execution/executionTypes.ts`
- `apps/desktop/src/features/execution/CommandActivityRow.tsx`
- `apps/desktop/src/features/execution/CommandApprovalBar.tsx`
- `apps/desktop/src/features/execution/TerminalOutputView.tsx`
- `apps/desktop/src/features/execution/GitChangesView.tsx`
- `apps/desktop/src/styles/execution.css`
- `apps/desktop/tests/c4-execution-primitives.test.tsx`
- `docs/execution/2026-08-02-wave-c4-d1-grok3-ui-primitives-development-report.md`

Do not mount components or change global/product entry files. Do not stage or
commit. Run the Task 4 RED/GREEN gates, report test counts, typecheck/build,
accessibility/privacy checks, changed files and risks, then send one Keeper
signal.
