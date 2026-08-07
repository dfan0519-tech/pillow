# C4 D3B4 Round 2B — Grok4 narrow repair brief

Date: 2026-08-05  
Implementer: actual Grok CLI slot `grok4`  
Controller verdict on initial delivery: **REJECTED pending this repair**

## Read first

1. `<repo>\docs\execution\2026-08-05-wave-c4-d3b4-grok4-git-workbench-brief.md`
2. `<repo>\docs\execution\2026-08-05-wave-c4-d3b4-grok4-git-workbench-report.md`
3. `<repo>\apps\desktop\src\features\conversations\approvalArbitration.ts`
4. `<repo>\apps\desktop\tests\approval-arbitration.test.ts`
5. current versions of the allowlisted files below

Keeper is record-only. Attempt worker boot once; if parent-session validation
rejects it, record that fact and continue without impersonating controller.

## Exact write allowlist

- Modify `apps/desktop/src/App.tsx`
- Modify `apps/desktop/tests/c4-git-changes-integration.test.tsx`
- Append repair evidence to
  `docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-report.md`

Do not edit Workbench, main, gateways, arbitration, any other test, package
files, plans, or unrelated dirty work. Do not stage or commit.

## Controller findings to repair

### Finding 1 — Important: current-project Git model lacks a render-time owner guard

The project-change effect increments `gitScopeRef` and clears Git state only
after React commits the render for the new `activeProject`. During that render,
`gitChanges` is still built from the previous project's `gitState`. Effects and
late-result guards do not prove that an old model cannot be passed for one
render/paint.

Add an explicit render-time ownership condition: `gitChanges` must be `null`
unless the active project id matches the project id currently owning the Git
scope/state. Initial selection should remain null until the project-change
effect establishes the new scope and loading state. Do not add polling, layout
effects, native paths, or a second project state machine.

Add a regression test that records every App-to-Workbench Git model render (or
an equivalently direct observable contract) and proves that once Project B is
the active project no Project A repository/status/diff model is supplied, even
before B's deferred discovery settles. A DOM assertion hidden by Workbench pane
closure is not sufficient.

### Finding 2 — Test fixture contradicts accepted approval arbitration

Fresh controller command:

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/c4-git-changes-integration.test.tsx --no-cache
```

Result: 11 passed / 1 failed. The failing precedence test lists one actionable
write and one actionable process request, then waits for `write-approval-bar`.
The accepted arbitration chooses by expiry then opaque request id; with the
fixture's near-equal timestamps and ids, the process request legitimately wins.

Repair the test, not arbitration/product behavior. Make the write-Diff portion
write-only (or give it an unambiguous accepted arbitration ordering), then make
the Terminal portion process-only. Preserve assertions that Terminal and write
Diff each outrank Git Changes. Use fixed timestamps, not `new Date()` ordering,
sleeps, or polling.

## Required gates

Run serially and record exact counts/exit codes:

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/c4-git-changes-integration.test.tsx --no-cache
pnpm --filter @pillow/desktop exec vitest run tests/approval-arbitration.test.ts tests/c4-git-changes-integration.test.tsx --no-cache
pnpm --filter @pillow/desktop typecheck
git diff --check -- apps/desktop/src/App.tsx apps/desktop/tests/c4-git-changes-integration.test.tsx docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-report.md
git diff --cached --name-only
```

Append a `Narrow repair` section to the existing report describing both fixes,
fresh commands, counts, residual risk, and staging state. End stdout and the
report with:

`C4-D3B4-GROK4-GIT-WORKBENCH-REPAIR-COMPLETE`

Worker completion remains subject to controller source review and fresh gates.
