# C4 D3B1 Grok4 brief — process events and conversation projection

Read this file first. It is the complete task contract.

You are an actual Keeper `grok4` approved-write worker. Work only in
`<repo>`; Keeper and reference repositories are read-only. Preserve all
unrelated work. Do not stage, commit, reset, restore, clean, package, or
publish. The approved-write wrapper may not run terminal commands; implement
and report the exact controller test commands when terminal access is denied.

## Objective

Carry the four accepted durable process event kinds through the TypeScript
conversation boundary and activity projection:

- `process_proposal_requested`
- `process_proposal_decided`
- `process_started`
- `process_terminal_outcome`

Extend strict gateway normalization and projection without exposing grants,
raw run/session ids, host paths, or debug metadata. Activity must be owned by
its conversation and summarize progressively: proposal/decision/start/terminal
state first; output remains pull-on-demand through `ExecGateway`, not persisted
in timeline payloads. Unknown or malformed payloads must fail closed according
to the existing gateway conventions. Add focused TS contract/projection tests,
including conversation isolation and terminal normalization.

## Exact write allowlist

- `apps/desktop/src/lib/conversationGateway.ts`
- `apps/desktop/src/lib/tauriConversationGateway.ts`
- `apps/desktop/src/features/conversations/agentActivityFromTimeline.ts`
- `apps/desktop/src/features/conversations/agentActivityTypes.ts`
- `apps/desktop/src/features/conversations/agentActivityProjection.ts`
- `apps/desktop/tests/c4-gateway-contract.test.ts`
- `apps/desktop/tests/tauri-conversation-gateway.test.ts`
- `apps/desktop/tests/agent-activity-timeline-integration.test.tsx`
- `docs/execution/2026-08-04-wave-c4-d3b1-grok4-process-event-projection-report.md`

No other file may change. In particular, do not edit `execGateway.ts`,
`tauriExecGateway.ts`, Workbench/React/CSS/E2E/native/Rust files or accepted
process authority.

## Completion contract

Describe changed files, test commands for the controller, risks, HEAD/staging
status, and end with `COMPLETION: complete|blocked`. Post a Keeper completion
signal before exit. Controller integration and acceptance remain separate.
