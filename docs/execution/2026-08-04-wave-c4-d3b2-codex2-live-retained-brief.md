# C4 D3B2 Codex2 brief — live-retained output truth

Read this file first. Work in `<repo>`. Preserve all unrelated dirty
work. Do not stage, commit, reset, restore, clean, package, or publish.

## Objective

Add the host-owned `ProcessOutputView.live_retained` truth to the existing
native and TypeScript output DTO as required `liveRetained: boolean`. This is
presentation metadata only, never authority. It lets D3B distinguish a real
empty live transcript from output that was not retained after restart.

The strict normalizer must require exactly this key and reject missing,
wrong-type, or authority/debug extras. Do not change output caps, execution,
ledger, approval, events, runtime host, process executor, or UI.

## Exact write allowlist

- `apps/desktop/src-tauri/src/conversation_service/exec_service.rs`
- `apps/desktop/src-tauri/src/exec_commands_tests.rs`
- `apps/desktop/src/lib/execGateway.ts`
- `apps/desktop/src/lib/tauriExecGateway.ts`
- `apps/desktop/tests/c4-gateway-contract.test.ts`
- `apps/desktop/tests/tauri-conversation-gateway.test.ts` only if fixture parity
  requires it
- `docs/execution/2026-08-04-wave-c4-d3b2-codex2-live-retained-report.md`

No other file may change.

## Required evidence

Run focused Rust serialization/command tests, focused TS gateway tests, and
desktop typecheck. Report changed files, commands, exact pass counts, residual
risk, and end with `COMPLETION: complete|blocked`. Post a Keeper completion
message before exit. Controller acceptance remains separate.
