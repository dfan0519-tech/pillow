# C4 D3B1 Grok4 brief — single approval-bar arbitration

Read this file first. It is the complete task contract.

You are an actual Keeper `grok4` approved-write worker. Work only in
`<repo>`; Keeper and references are read-only. Preserve unrelated work.
Do not stage, commit, reset, restore, clean, package, or publish. The wrapper
may deny terminal commands; implementation plus a precise controller test plan
is sufficient when that occurs.

## Objective

Implement a small pure TypeScript arbitration model that selects exactly one
visible approval request across governed writes and governed processes. The
selection rule is fixed:

1. Include only requests whose decision is currently available.
2. Choose the oldest expiry timestamp.
3. Break equal-expiry ties by stable opaque request id.
4. If expiry and id are equal, write approval wins.

The returned renderer model must contain only the selected domain, opaque id,
conversation id, expiry, and presentation-safe summary/risk fields. It must not
contain a write grant, process authority, raw run/session id, host path, or
debug metadata. The helper must be deterministic, conversation-scoped, and
return no selection for a project/conversation mismatch. Add exhaustive unit
tests for ordering, tie-breaks, isolation, empty inputs, unavailable decisions,
and input immutability. Do not mount UI in this slice.

## Exact write allowlist

- `apps/desktop/src/features/conversations/approvalArbitration.ts` (new)
- `apps/desktop/tests/approval-arbitration.test.ts` (new)
- `docs/execution/2026-08-04-wave-c4-d3b1-grok4-approval-arbitration-report.md`

No other file may change. Do not edit existing gateways, projections,
Workbench/React/CSS/E2E/native/Rust files, write policy, or process authority.

## Completion contract

Describe changed files, test commands for the controller, risks, HEAD/staging
status, and end with `COMPLETION: complete|blocked`. Post a Keeper completion
signal before exit. Controller integration and acceptance remain separate.
