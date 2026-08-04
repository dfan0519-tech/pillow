# C4 D3AR2C Grok4 brief — desktop event integration repair

Read this file first. It is the complete task contract.

You are the actual Keeper `grok4` CLI worker. Boot Keeper as a `pillow` worker
for task `c4-d3ar2c-desktop-event-integration-repair`. Work only in
`<workspace>\pillow`; Keeper is read-only. Preserve HEAD
`7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`, empty staging and unrelated dirty
work. No stage, commit, reset, restore, clean, package or publish.

## Controller rejection evidence

After the adapter repair, controller reran `pnpm check`. UI tests passed
272/272, then the Rust workspace failed on exactly two desktop compilation
gaps:

1. `conversation_service/pipeline.rs::kind_str` is non-exhaustive for the four
   fixed process event kinds.
2. `exec_commands.rs` has an existing RED test for `is_exec_public_error` but
   no implementation.

Repair only these compile/integration gaps. In `kind_str`, add explicit arms
for the four exact snake_case names; do not add a wildcard. In
`exec_commands.rs`, implement a private fixed public-error validator matching
the existing TypeScript `EXEC_PUBLIC_ERROR_CODES` vocabulary in
`apps/desktop/src/lib/execGateway.ts`. Do not implement Tauri commands or widen
D3B product scope in this repair.

## Exact write allowlist

- `apps/desktop/src-tauri/src/conversation_service/pipeline.rs`
- `apps/desktop/src-tauri/src/exec_commands.rs`
- `docs/execution/2026-08-04-wave-c4-d3ar2c-grok4-desktop-event-integration-repair-report.md`

No other file may change.

## Required gates

```text
cargo test -p pillow-desktop --all-targets
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
git diff --check -- apps/desktop/src-tauri/src/conversation_service/pipeline.rs apps/desktop/src-tauri/src/exec_commands.rs
git diff --cached --name-only
git rev-parse HEAD
```

Write the report path above with exact changes, tests, risks, HEAD/staging and
Keeper completion ID. Release only your exact lease and exit. Controller will
independently rerun `pnpm check`, production UI build and desktop E2E.
