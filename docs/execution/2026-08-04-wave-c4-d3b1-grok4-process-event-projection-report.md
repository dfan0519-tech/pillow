# C4 D3B1 process event projection report

Date: 2026-08-04
Worker: `codex2-d3b1-events-takeover`
Keeper session: `<session-id>`
Parent session: `<session-id>`
Brief: `docs/execution/2026-08-04-wave-c4-d3b1-grok4-process-event-projection-brief.md`

## Delivered

The TypeScript conversation boundary now accepts only the four durable process
event kinds:

- `process_proposal_requested`
- `process_proposal_decided`
- `process_started`
- `process_terminal_outcome`

Each payload is exact-key normalized at the Tauri boundary. The parser rejects
unknown fields, grants, output/transcript fields, native paths, invalid enums,
oversized argument vectors, invalid timestamps, and invalid terminal metadata.
Process output is not put into timeline/activity data and remains
pull-on-demand through `ExecGateway`.

Activity mapping filters events by both owning conversation and run, retains
only internal proposal IDs plus phase/state metadata, orders them by durable
sequence, and projects fixed path-free lifecycle summaries first. Proposal,
run, session, program, argv, cwd, and output content do not enter rendered
copy. Process terminal outcomes remain non-terminal for the conversation run.

## Changed files

| Path | Change |
| --- | --- |
| `apps/desktop/src/lib/conversationGateway.ts` | Added typed process event kinds and payload DTOs. |
| `apps/desktop/src/lib/tauriConversationGateway.ts` | Added strict exact-key process payload normalization. |
| `apps/desktop/src/features/conversations/agentActivityTypes.ts` | Added internal process lifecycle activity input. |
| `apps/desktop/src/features/conversations/agentActivityFromTimeline.ts` | Added ordered, conversation-owned process event mapping and ID sanitization. |
| `apps/desktop/src/features/conversations/agentActivityProjection.ts` | Added fixed, progressive lifecycle summaries before ordinary notices. |
| `apps/desktop/tests/c4-gateway-contract.test.ts` | Proved process terminal metadata does not terminate a conversation run. |
| `apps/desktop/tests/tauri-conversation-gateway.test.ts` | Covered all four valid payloads and fail-closed malformed/secret/output/native-path cases. |
| `apps/desktop/tests/agent-activity-timeline-integration.test.tsx` | Covered durable order, conversation isolation, terminal normalization, and non-disclosure in rendered activity. |
| `docs/execution/2026-08-04-wave-c4-d3b1-grok4-process-event-projection-report.md` | This report. |

## Checks run

| Command | Result |
| --- | --- |
| `pnpm --filter @pillow/desktop typecheck` | PASS |
| `pnpm --filter @pillow/desktop test -- c4-gateway-contract.test.ts tauri-conversation-gateway.test.ts agent-activity-timeline-integration.test.tsx` | PASS — dispatcher ran 24 files / 281 tests. Expected negative-path test diagnostics were emitted to stderr. |
| `git diff --check -- <allowlisted TypeScript and test files>` | PASS |

## Controller commands

Controller integration and acceptance remain separate. Run:

```powershell
pnpm check
pnpm desktop:smoke:dev
pnpm --filter @pillow/desktop exec vitest run tests/c4-gateway-contract.test.ts tests/tauri-conversation-gateway.test.ts tests/agent-activity-timeline-integration.test.tsx
```

## Risks and residual scope

- No process command UI was mounted and no process authority, native bridge,
  `ExecGateway`, or Rust code was edited.
- Timeline summaries intentionally contain no transcript, raw IDs, grant,
  program/argv/cwd, native path, or debug metadata. Full live output remains
  an `ExecGateway` concern.
- Existing unrelated native-bridge and approval-arbitration work was preserved.
- Vault ingest is not performed because the execution brief's exact write
  allowlist forbids changes outside the listed report/code/test files.

## HEAD and staging

- HEAD: `62149be9eddd2288eb45fd9b8278641cf64c7a5f`
- Staging: empty before and after; nothing staged or committed.
- Working tree: pre-existing unrelated native/Rust and untracked execution
  files remain; this worker changed only the brief allowlist.

## Keeper completion

- Posted to parent session `<session-id>`.
- Message: `msg-20260804T112439493Z-20260804t111805z-pillow-t2mj`

COMPLETION: complete
