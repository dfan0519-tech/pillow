# Wave C2-C2 — In-Conversation Agent Activity Disclosure

## Scope and source rule

Implement an isolated renderer component slice. It must consume activity data
provided by the owning conversation; it must not fabricate progress, poll a
global background-task table, expose raw session IDs, or create a second
dashboard.

The interaction idea may be informed by local `zbbody` (Apache-2.0,
conceptual-only) and Grok App commit `cbbbdf17bb8db1588b6707f3f42f6b00397e27f9`
(MIT, provenance required for any direct adaptation). Do not copy source from
either reference; record provenance in the completion report.

## Exact write whitelist

- `apps/desktop/src/features/conversations/AgentActivityDisclosure.tsx` (new)
- `apps/desktop/src/features/conversations/agentActivityProjection.ts` (new)
- `apps/desktop/src/features/conversations/agentActivityTypes.ts` (new)
- `apps/desktop/src/styles/agent-activity.css` (new)
- `apps/desktop/tests/agent-activity-disclosure.test.tsx` (new)
- `docs/execution/2026-07-31-wave-c2c2-agent-activity-report.md` (new)

Do not edit existing `ConversationTimeline.tsx`, `RunActivity.tsx`, App or
workbench wiring, existing CSS, Tauri, provider/runtime, or C2-A/B code. Do
not stage or commit.

## Contract

Props accept `conversationId`, a run/event/tool/sub-agent activity input, and
an optional `onOpenArtifact` callback. They only render activity belonging to
the supplied conversation ID.

- Summary line is compact and human-readable (e.g. role/status/current step),
  with no raw IDs or diagnostic metadata.
- Details are closed by default. Expanded detail shows ordered steps and role
  state, then optional output/file/test results as nested progressive
  disclosure.
- Tool/sub-agent entries must derive from actual supplied records. Missing
  inputs render no invented work.
- Terminal/failure/cancelled state remains visible and accessible; successful
  history is concise and does not stack permanent cards.
- File/artifact callbacks receive portable relative paths only; errors are
  opaque and do not leak paths.

## Test-first and verification

Create the focused test first and run RED. Cover ownership filtering, compact
summary, closed-default expansion, ordered actual tool/sub-agent steps,
terminal accessibility, no raw session IDs, and portable artifact callbacks.

Run:

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/agent-activity-disclosure.test.tsx
pnpm --filter @pillow/desktop typecheck
git diff --check
```
