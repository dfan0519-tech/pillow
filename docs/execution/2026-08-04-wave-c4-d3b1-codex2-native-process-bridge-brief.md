# C4 D3B1 Codex2 brief — native process bridge

Read this file first. It is the complete task contract.

You are the actual account-2 Codex CLI worker (`CODEX_HOME=%USERPROFILE%\.codex2`,
model `gpt-5.6-terra`, reasoning `high`, `fastMode=false`). Boot Keeper for
project `pillow` as worker `codex2-d3b1-native`. Work only in
`<repo>`. Preserve all unrelated work and do not stage, commit, reset,
restore, clean, package, publish, or modify Keeper/reference repositories.

## Objective

Implement the product Tauri bridge for the already accepted D3AR2 governed
process authority. Expose exactly the existing renderer contract command names:

- `list_exec_proposals`
- `decide_exec_proposal`
- `open_exec_output`

The commands must delegate to the existing `RuntimeHost`/governed-process
authority. Do not create a second proposal store, executor, grant, output
transcript, or policy layer. Project/conversation ownership is mandatory;
renderer output remains authority-free (`decisionAvailable` only). Public
errors must stay within `EXEC_PUBLIC_ERRORS`. Register the three commands in
the product invoke handler and add focused Rust tests for serialization,
ownership rejection, invalid decisions, unavailable output, and registration.

Use test-driven development. Run focused Rust tests, then `cargo test -p
pillow-desktop --all-targets`, `cargo fmt --all --check`, and scoped
`git diff --check`. If a full workspace failure is outside the allowlist,
report it accurately and stop rather than expanding scope.

## Exact write allowlist

- `apps/desktop/src-tauri/src/exec_commands.rs`
- `apps/desktop/src-tauri/src/conversation_service/exec_service.rs` (new)
- `apps/desktop/src-tauri/src/conversation_service/mod.rs`
- `apps/desktop/src-tauri/src/conversation_service/service.rs`
- `apps/desktop/src-tauri/src/lib.rs`
- `apps/desktop/src-tauri/src/write_commands_tests.rs` only if the existing
  shared registration harness cannot cover the new commands without it
- `apps/desktop/src-tauri/src/exec_commands_tests.rs` (new, preferred)
- `docs/execution/2026-08-04-wave-c4-d3b1-codex2-native-process-bridge-report.md`

No other file may change. In particular, do not edit process protocol, policy,
executor, storage, runtime-host, TypeScript, React, CSS, or E2E files.

## Completion contract

Report changed files, focused tests, residual risks, HEAD/staging status, and
`COMPLETION: complete|blocked`. Post the same completion signal through Keeper
bridge before exiting. Controller integration and acceptance remain separate.
