# C4 D3B2 Grok4 brief — App process refresh composition

Read this file first. Work in `<repo>`. Preserve all unrelated dirty
work. Do not stage, commit, reset, restore, clean, package, or publish.

## Objective

Wire `ExecGateway` through renderer composition and bump exactly one
event-driven refresh token for the four accepted process lifecycle kinds when
the event belongs to the currently selected conversation under the active
project. Keep the existing write refresh path unchanged. No polling, process
state cache, grant, output, native path, Workbench implementation, or UI
styling belongs in this slice.

The exact Workbench-facing props are:

```ts
execGateway?: ExecGateway;
execProposalRefreshToken?: number;
```

If the current Workbench does not yet declare them, do not edit Workbench.
Document that expected temporary typecheck dependency in the report; the
controller will integrate after both non-overlapping slices complete.

## Exact write allowlist

- `apps/desktop/src/App.tsx`
- `apps/desktop/src/main.tsx`
- `apps/desktop/tests/c4-app-process-refresh.test.tsx`
- `docs/execution/2026-08-04-wave-c4-d3b2-grok4-app-refresh-report.md`

No other file may change.

## Required evidence

Test all four kinds, foreign conversation, foreign project, and write-event
regression. Run the focused test and typecheck when the shared tree permits.
Report changed files, commands, pass/fail truth, residual dependency, and end
with `COMPLETION: complete|blocked`. Post a Keeper completion signal before
exit. Controller acceptance remains separate.
