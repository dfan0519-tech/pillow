# C4 D3B2 Grok2 — compact conversation-owned command activity report

Date: 2026-08-04  
Worker: grok2  
Brief: `docs/execution/2026-08-04-wave-c4-d3b2-grok2-process-activity-brief.md`

## Delivered

Replaced the four generic process lifecycle notice paragraphs with one compact,
conversation-owned command activity row per real proposal:

- Pure `processActivityModel.ts` groups durable `RuntimeEventDto` process
  events by internal `proposalId`, filters by owning conversation + run, orders
  by durable sequence, and derives fixed safe Chinese status copy.
- Summary shows program label + lifecycle status only. Completed success uses
  the concise collapsed disclosure class.
- Expanded details show redacted-safe argv, decision, exit code, duration,
  captured bytes, truncated/lossy notices, and `完整输出` when the process has
  started or reached a terminal outcome.
- Native absolute / UNC path-looking argv tokens are redacted to `[path]`;
  portable relative tokens remain literal. Portable cwd, raw IDs, grants,
  digests, transcripts, and host debug fields never enter the row DOM.
- `完整输出` routes the internal proposal id plus safe program/argv into the
  existing Workbench Terminal output target / `ResourcePane` (pull-on-demand
  via `ExecGateway`). No second dashboard, polling, process cache, or alternate
  output renderer.
- Removed the old `processActivity` notice path from agent activity mapping and
  projection. Write outcomes and all non-process agent activity remain.

## Exact changed files

| Path | Change |
| --- | --- |
| `apps/desktop/src/features/execution/processActivityModel.ts` | **Created** pure grouper + argv redaction + safe status labels |
| `apps/desktop/src/features/conversations/ConversationTimeline.tsx` | Mount command activity rows; progressive disclosure; open-output callback |
| `apps/desktop/src/features/workbench/Workbench.tsx` | Wire timeline open-output to existing Terminal target |
| `apps/desktop/src/features/conversations/agentActivityFromTimeline.ts` | Stop mapping process events into agent activity |
| `apps/desktop/src/features/conversations/agentActivityProjection.ts` | Remove generic process notice projection |
| `apps/desktop/src/features/conversations/agentActivityTypes.ts` | Drop `AgentProcessActivityInput` / `processActivity` record field |
| `apps/desktop/tests/agent-activity-timeline-integration.test.tsx` | Replace generic process-notice assertions |
| `apps/desktop/tests/c4-process-activity-integration.test.tsx` | **Created** focused TDD integration suite |
| `docs/execution/2026-08-04-wave-c4-d3b2-grok2-process-activity-report.md` | This report |

No CSS files changed. Existing `command-activity-row*` classes are reused.

## TDD evidence

### RED (tests first)

1. `pnpm --filter @pillow/desktop exec vitest run tests/c4-process-activity-integration.test.tsx tests/agent-activity-timeline-integration.test.tsx`
   - FAIL: missing `processActivityModel` import; agent activity still emitted four process notice phases.
   - Suite load failed for new file; agent-activity process assertion failed as expected.

### GREEN (required commands)

1. `pnpm --filter @pillow/desktop exec vitest run tests/c4-process-activity-integration.test.tsx tests/agent-activity-timeline-integration.test.tsx tests/c4-workbench-process-integration.test.tsx`
   - **PASS**: 3 files, **58 tests passed** (11 process-activity + 25 agent-activity + 22 workbench), 0 failed.
2. `pnpm --filter @pillow/desktop typecheck`
   - **PASS**: `tsc --noEmit` exit 0.
3. `git diff --check -- <allowlisted code/test/report files>`
   - **PASS**: exit 0 (CRLF normalization warnings only; no whitespace errors).

### Coverage proven

1. proposal/decision/start/terminal group into one ordered row per proposal  
2. safe program/argv and terminal metadata only after disclosure  
3. successful commands remain concise/collapsed  
4. `完整输出` routes exact internal proposal to existing Terminal pane  
5. raw IDs, native paths, cwd, grants, digests, transcript/debug fields absent from DOM  
6. foreign conversation/run events do not paint  
7. old four generic process notice paragraphs gone; non-process agent activity and write outcomes unchanged  

## Residual risks

- Inline output tail remains intentionally omitted; full transcript is still
  pull-on-demand through `ExecGateway` only.
- Origin is fixed to agent for durable process events (no user-origin process
  event field in the normalized payloads).
- Controller acceptance, desktop smoke, and broader monorepo gates remain
  separate.
- No Keeper/Vault context pack was requested; this brief was the sole task
  authority. Staging and commit were not performed.

## HEAD and staging

- HEAD at report time: `62149be9eddd2288eb45fd9b8278641cf64c7a5f`
- Staging: empty (nothing staged or committed by this worker)
- Working tree: only allowlisted files touched for this task; unrelated dirty
  work preserved

COMPLETION: complete
