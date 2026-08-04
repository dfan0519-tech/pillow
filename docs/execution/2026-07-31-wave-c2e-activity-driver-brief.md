# Wave C2-E Activity Driver Worker Brief

Read first and treat as the exact requirements:
`docs/superpowers/plans/2026-07-31-c2e-isolation-restart-desktop-acceptance.md`.

## Role

You are grok2. Own only the real conversation/activity UI helper. The
orchestrator and resource helper are parallel-owned; do not edit them.

## Exclusive write whitelist

- `scripts/lib/c2-activity-ui-acceptance.mjs`
- `docs/execution/2026-07-31-wave-c2e-activity-driver-report.md`

Do not edit any other path.

## Required exports

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

## Required behavior

- First boot Keeper as a worker for project `pillow`, task
  `wave-c2e-activity-driver`.
- Use accessible roles/names and existing stable `data-testid` values only.
- Create the conversation through `新建对话`, send through the composer, and
  wait for terminal assistant output.
- Expand the real `agent-activity-disclosure`; assert plan, tool, sub-agent,
  output/artifact disclosure, and completed state from the committed fake
  runtime fixture.
- Restore assertions must not send another turn.
- Absence checks cover foreign prompt and foreign conversation-owned activity.
- Do not read SQLite/filesystem or expose raw run/session/tool/sub-agent IDs.
- Helpers do not select projects, launch/close processes, or write files.

## Focused verification

Use a syntax/import smoke if the outer orchestrator is not ready. Run
`git diff --check` on the whitelist.

## Completion contract

Write the report file with changed files, evidence assertions, tests, residual
risks, and status. Return only status, one-line test summary, concerns, and the
Keeper completion message. Worker completion is not controller acceptance.
